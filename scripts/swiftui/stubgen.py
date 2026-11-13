#!/usr/bin/env python3
"""Turn a .swiftinterface into Swift source that compiles to the same ABI.

The interface lists every public (and @usableFromInline, @_spi, package)
declaration with its exact signature and attributes; functions, initializers
and accessors that aren't inlinable have no bodies. This gives each one a body
that stops with its name, so a module built from the result exports the same
symbols as the original. pomme's own implementations then replace the stubs,
one API at a time (see scripts/swiftui/README.md).

Kept as written: protocol requirements, inlinable bodies, stored properties,
enum cases, typealiases, and attributes such as @_originallyDefinedIn, which
decide how symbols are mangled.

Usage: stubgen.py <interface> <output.swift> <processed dir> [pomme's sources…]
       stubgen.py --markers <interface> <regex>   (print matching markers)

pomme's sources take declarations over with markers (see read_markers); they're
compiled from <processed dir>.
"""
import re
import sys

ACCESSOR = re.compile(
    r'^(\s*)((?:@\w+\s+)*(?:mutating |nonmutating |__consuming |borrowing |consuming )*)'
    r'(get|set|_modify|_read|unsafeAddress|unsafeMutableAddress|didSet|willSet)\s*(async\s*)?((?:throws(?:\([^)]*\))?)\s*)?$')
TYPE_OPEN = re.compile(r'\b(protocol|struct|class|enum|actor|extension)\b')
MODIFIERS = re.compile(
    r'^((public|internal|package|private|fileprivate|open|final|static|class|override|required|convenience|nonisolated|'
    r'mutating|nonmutating|dynamic|optional|indirect|__consuming|borrowing|consuming|isolated|distributed|prefix|postfix|infix)'
    r'(\([\w ]*\))?\s+)*')

# Protocols Swift.Never conforms to in SwiftUI: an opaque result of one of
# these can be a stop (Never) itself.
NEVER_CONFORMS = {
    'View', 'Scene', 'Gesture', 'ShapeStyle', 'Keyframes', 'CustomHoverEffect', 'Commands', 'ToolbarContent',
    'CustomizableToolbarContent', 'TableColumnContent', 'TableRowContent', 'AccessibilityRotorContent',
    'WidgetConfiguration', 'ImmersiveSpaceContent', 'CompositorContent', 'ControlWidgetConfiguration',
    'ControlWidgetTemplate',
}
IOS_UNAVAILABLE = re.compile(r'@available\(iOS, unavailable\b|@available\(iOS(?:ApplicationExtension)?, unavailable')

# Stored properties pomme gives a type (its own storage: the interface
# doesn't show a resilient type's). Inserted at the top of the type's body.
STORAGE = {
    'Spring': ['internal var _duration: Swift.Double', 'internal var _bounce: Swift.Double'],
}

# Standard library protocols: a concrete type stands in.
STANDARD_STAND_INS = {'Equatable': 'Swift.Int', 'Hashable': 'Swift.Int'}


def strip_strings(line):
    return re.sub(r'"(?:\\.|[^"\\])*"', '""', line)


def braces(line):
    s = re.sub(r'//.*', '', strip_strings(line))
    return s.count('{') - s.count('}')


def decl_keyword(line):
    """func / init / subscript / deinit when the line declares one."""
    s = strip_strings(line).strip()
    if s.startswith('//') or s.startswith('#'):
        return None
    t = re.sub(r'^(@[\w.]+(\((?:[^()]|\([^()]*\))*\))?\s*)*', '', s)
    t = MODIFIERS.sub('', t)
    m = re.match(r'(func|init|subscript|deinit)\b', t)
    return m.group(1) if m else None


def split_at_body(line):
    """(signature, rest): rest starts at the declaration's own '{', if any.
    Braces inside parentheses (default-argument closures) don't count."""
    s = strip_strings(line)
    depth = 0
    for i, c in enumerate(s):
        if c in '([':
            depth += 1
        elif c in ')]':
            depth -= 1
        elif c == '{' and depth == 0:
            return line[:i], line[i:]
    return line, None


def result_type(signature):
    """The declared result type after '->' (functions) or ':' (properties)."""
    s = strip_strings(signature).replace('->', '\x00\x00')  # arrows aren't angle brackets
    depth = 0
    for i, c in enumerate(s):
        if c in '(<[':
            depth += 1
        elif c in ')>]':
            depth -= 1
        elif depth == 0 and s.startswith('\x00\x00', i):
            t = strip_strings(signature)[i + 2:]
            return re.split(r'\swhere\s', t)[0].strip()
    return None


def property_type(signature):
    s = strip_strings(signature)
    m = re.search(r'\b(?:var|let)\s+[`\w]+\s*:\s*(.+)$', s)
    return m.group(1).strip() if m else None


def opaque_stand_in(ty):
    """For an opaque result type ('some P<Args>'): None when Never can be it,
    else the type to stand in for it."""
    if not ty or not ty.startswith('some '):
        return None
    m = re.match(r'some\s+([\w.]+)(<.*>)?', ty)
    if not m:
        return None
    proto = m.group(1).split('.')[-1]
    args = m.group(2) or ''
    # Never can't stand in when the result fixes a primary associated type.
    if proto in NEVER_CONFORMS and not args:
        return None
    if proto in STANDARD_STAND_INS:
        return STANDARD_STAND_INS[proto]
    if proto == 'Sequence':
        return f'[{args[1:-1]}]' if args else '[Swift.Int]'
    if proto == 'TransferRepresentation' and not args:
        return '_PommeOpaqueTransferRepresentation<Self>'  # a Transferable's own: its item is itself
    if proto == 'IteratorProtocol':
        return f'Swift.IndexingIterator<[{args[1:-1]}]>' if args else 'Swift.IndexingIterator<[Swift.Int]>'
    return f'_PommeOpaque{proto}{args}'


def split_top(s, sep=','):
    """Split at `sep` outside brackets."""
    parts, depth, cur = [], 0, ''
    for c in s.replace('->', '\x00'):
        if c in '([<{':
            depth += 1
        elif c in ')]>}':
            depth -= 1
        if c == sep and depth == 0:
            parts.append(cur)
            cur = ''
        else:
            cur += c
    parts.append(cur)
    return [p.replace('\x00', '->') for p in parts if p.strip()]


def super_call(sig):
    """For an overriding method: a call of the superclass's (the arguments
    passed on as they came), or None when it can't be written (variadics)."""
    m = re.search(r'\bfunc\s+([`\w]+)\s*(<[^(]*>)?\s*\(', sig)
    if not m:
        return None
    start = m.end()
    depth, i = 1, start
    while depth:
        depth += {'(': 1, ')': -1}.get(sig[i], 0)
        i += 1
    params, after = sig[start:i - 1], sig[i:]
    args = []
    for param in split_top(params):
        names, _, ty = param.partition(':')
        names = names.split()
        if '...' in ty:
            return None
        external, internal = (names[0], names[-1])
        value = ('&' if re.match(r'\s*inout\b', ty) else '') + internal
        args.append(value if external == '_' else f'{external}: {value}')
    call = f'super.{m.group(1)}({", ".join(args)})'
    if re.search(r'\basync\b', after.split('->')[0]):
        call = 'await ' + call
    if re.search(r'\b(throws|rethrows)\b', after.split('->')[0]):
        call = 'try ' + call
    return call


def stub_body(what, ty):
    """A body that stops. Results are an explicit `return`, which also keeps a
    result builder (one the declaration or its requirement has) out of it."""
    name = re.sub(r'\s+', ' ', what)[:120].replace('\\', '\\\\').replace('"', '\\"')
    if not ty or ty in ('Swift.Void', 'Void', '()'):
        return f'{{ _pommeUnimplemented("{name}") }}'
    stand_in = opaque_stand_in(ty)
    if ty.startswith('some ') and stand_in is None:
        return f'{{ return _pommeUnimplemented("{name}") }}'
    if stand_in:
        return f'{{ return _pommeUnimplementedValue("{name}") as {stand_in} }}'
    return f'{{ return _pommeUnimplementedValue("{name}") }}'


def nonisolated_if_concurrent(line, sig, kind):
    """A member taking concurrency-annotated functions (`@isolated(any)`,
    `@Sendable`), declared `nonisolated` unless it says its isolation.

    Without that, a member of a type that conforms to a @preconcurrency
    main-actor protocol (View, ViewModifier) is inferred main-actor isolated,
    and Swift mangles such a declaration without those annotations. Apple's
    binary has them (the task modifiers' initializers): its members aren't
    isolated, which the interface doesn't show."""
    if kind != 'type' or not re.search(r'@isolated\(any\)|@Sendable', sig):
        return line
    head = re.split(r'\b(?:func|init|subscript)\b', sig)[0]  # attributes and modifiers
    if re.search(r'\bnonisolated\b|@MainActor\b|@\w+Actor\b', head):
        return line
    return re.sub(r'^(\s*(?:@[\w.]+(?:\((?:[^()]|\([^()]*\))*\))?\s+)*)', r'\1nonisolated ', line, count=1)


ATTRIBUTES = re.compile(r'^(@[\w.]+(\((?:[^()]|\((?:[^()]|\([^()]*\))*\))*\))?\s*)*')


def key(decl):
    """A declaration as pomme's markers name it: its first line without
    attributes, body or brace, whitespace collapsed."""
    sig, _ = split_at_body(decl.strip())
    return re.sub(r'\s+', ' ', ATTRIBUTES.sub('', sig.strip())).rstrip(' {').strip()


def read_markers(paths, processed_dir=None):
    """pomme's takeovers, from its sources:
      // pomme: owns <type declaration>
      // pomme: replaces <type or extension declaration> :: <member declaration>
      // pomme: in <type declaration or interface extension>   (on an `extension … {` block)
    each written as `key` gives it (copy it from the generated source).

    An `in` block's contents go into the type's own declaration, in place of
    the interface's body, for what has to be there: stored properties, a
    property wrapper's `wrappedValue`, and members of SwiftUICore's types,
    which Swift mangles differently in an extension (the types were
    originally defined in SwiftUI). The sources are written to
    `processed_dir` without those blocks, to be compiled from there.

    On an interface extension (`extension SwiftUICore.View`), the block goes
    into the first one: an extension of a SwiftUICore type carries the type's
    `@_originallyDefinedIn`, which its members' mangling depends on too."""
    owns, replaces, inside = {}, {}, {}
    for path in paths:
        lines = open(path).read().split('\n')
        kept = []
        i = 0
        while i < len(lines):
            line = lines[i]
            m = re.match(r'\s*// pomme: (owns|replaces|in) (.+)$', line)
            where = f'{path}:{i + 1}'
            if m and m.group(1) == 'in':
                # The block: from the `extension … {` line to its closing brace.
                assert re.match(r'\s*extension\b.*\{\s*$', lines[i + 1]), f'{where}: `in` goes on an extension'
                depth, j, body = braces(lines[i + 1]), i + 2, []
                while depth > 0:
                    depth += braces(lines[j])
                    if depth > 0:
                        body.append(lines[j])
                    j += 1
                inside[key(m.group(2))] = (body, where)
                for k, b in enumerate(body):
                    r = re.match(r'\s*// pomme: replaces (.+)$', b)
                    if r:
                        ctx, _, member = r.group(1).partition(' :: ')
                        replaces[(key(ctx), key(member))] = f'{path}:{i + 3 + k}'
                kept.extend([f'// (moved into the declaration: {m.group(2)})'] + [''] * (j - i - 1))
                i = j
                continue
            if m and m.group(1) == 'owns':
                owns[key(m.group(2))] = where
            elif m:
                ctx, _, member = m.group(2).partition(' :: ')
                replaces[(key(ctx), key(member))] = where
            kept.append(line)
            i += 1
        if processed_dir:
            # Same line numbers as the source, for compiler messages.
            open(f'{processed_dir}/{path.rsplit("/", 1)[-1]}', 'w').write('\n'.join(kept))
    return owns, replaces, inside


def qualified_name(head, top, modules):
    """A type's name with the types it's nested in (`Axis.Set`): for an
    extension, the type it extends, without its module."""
    m = re.match(r'\s*extension\s+([\w.]+)', head)
    if m:
        parts = m.group(1).split('.')
        return '.'.join(parts[1:] if len(parts) > 1 and parts[0] in modules else parts)
    m = re.search(r'\b(?:struct|class|enum|actor|protocol)\s+(\w+)', head)
    name = m.group(1) if m else '?'
    return f'{top[4]}.{name}' if top[0] == 'type' and len(top) > 4 else name


def derived_body(k, cases, info=None):
    """`==`, `hash(into:)` and `init?(rawValue:)` of an enum without payloads,
    as Swift derives them."""
    if info and info.get('raw') and re.fullmatch(r'public init\?\(rawValue: [\w.]+\)', k):
        arms = ' '.join(f'case {v}: self = .{c}' for c, v in zip(cases, info['values']))
        return f'{{ switch rawValue {{ {arms} default: return nil }} }}'
    if re.fullmatch(r'public static func == \(a: [\w.]+, b: [\w.]+\) -> Swift\.Bool', k):
        pairs = ', '.join(f'(.{c}, .{c})' for c in cases)
        return f'{{ switch (a, b) {{ case {pairs}: return true; default: return false }} }}'
    if k == 'public func hash(into hasher: inout Swift.Hasher)':
        arms = ' '.join(f'case .{c}: hasher.combine({i})' for i, c in enumerate(cases))
        return f'{{ switch self {{ {arms} }} }}'
    return None


def camel_words(name):
    """`topLeading` → ['top', 'leading']."""
    return [w.lower() for w in re.findall(r'[a-z0-9]+|[A-Z][a-z0-9]*', name)]


SEEN = []  # every marker the interface allows, for --markers
OBJC_DEINIT = re.compile(r'@objc (?=(@[\w()_]+ )*deinit)')


def m_storage_needed(head, frozen_pending):
    """A resilient struct has storage the interface doesn't show. Without any,
    Swift would synthesize an init() that clashes with the struct's own."""
    m = re.search(r'\b(struct|class|enum|actor)\s+(\w+)', head)
    return bool(m and m.group(1) == 'struct' and not re.search(r'@frozen\b', head) and not frozen_pending)


def generate(src, owns=None, replaces=None, inside=None):
    """`owns`: types pomme declares itself (the interface's members become an
    extension); `replaces`: (context, member) pairs pomme implements (left out);
    `inside`: pomme's members of a type's own declaration. As `read_markers`
    returns them; each must match, or it's an error."""
    owns, replaces, inside = owns or {}, replaces or {}, inside or {}
    used = set()
    option_bits = {}  # by option set: its options' values so far
    enums = {}  # by qualified name: an enum's cases, and whether any has a payload
    # The module and those it imports (an extension's type is named without them).
    modules = set(re.findall(r'^(?:@_exported\s+)?import\s+(\w+)', src, re.M)) | set(re.findall(r'-module-name (\w+)', src[:2000]))
    out = []
    # Context stack entries: ('protocol',), ('type', key), ('var', result type),
    # ('pvar',) for a protocol's property requirements, ('body',) for code.
    stack = []
    projected = set()  # wrapped properties' projections, declared by their wrappers
    pending = []  # indexes in `out` of attribute lines waiting for their declaration
    frozen_pending = False
    missing_inits = False
    dropped = set()  # top-level types left out (unavailable on iOS)
    for raw in src.split('\n'):
        # (`@objc deinit`, as interfaces print it: implicit, and in source it
        # needs Foundation imported.)
        line = OBJC_DEINIT.sub('', raw.rstrip())
        stripped = line.strip()
        top = stack[-1] if stack else ('file',)
        kind = top[0]

        if kind == 'body':
            out.append(line)
            d = braces(line)
            if d > 0:
                stack.extend([('body',)] * d)
            for _ in range(-d if d < 0 else 0):
                stack.pop()
            continue

        # A declaration being dropped (a property wrapper's projection, which the
        # wrapper synthesizes): skip to its end.
        if kind == 'skip':
            d = braces(line)
            if d > 0:
                stack.extend([('skip',)] * d)
            for _ in range(-d if d < 0 else 0):
                stack.pop()
            continue

        if kind in ('var', 'pvar') and stripped == '}':
            out.append(line)
            stack.pop()
            continue

        # An init accessor (`@storageRestrictions(initializes: x) init(initialValue)`).
        m = re.search(r'@storageRestrictions\(initializes:\s*(\w+)[^)]*\)\s*init\((\w+)\)\s*$', stripped)
        if m and kind in ('var', 'type'):
            out.append(f'{line} {{ self.{m.group(1)} = {m.group(2)} }}')
            continue

        if kind == 'var':
            m = ACCESSOR.match(line)
            if m and '{' not in line:
                indent, mods, kw = m.group(1), m.group(2) or '', m.group(3)
                asy, thr = m.group(4) or '', m.group(5) or ''
                ty = top[1] if kw in ('get', 'unsafeAddress') else None
                overridden = top[2] if len(top) > 2 else None
                if overridden and overridden.startswith('#env:') and kw in ('get', 'set'):
                    value = overridden[5:]
                    if kw == 'get':
                        out.append(f'{indent}{mods}get {{ return pommeEnvironmentValue("{value}") }}')
                    else:
                        out.append(f'{indent}{mods}set {{ pommeSetEnvironmentValue("{value}", newValue) }}')
                elif overridden == '#hashValue' and kw == 'get':
                    out.append(f'{indent}{mods}get {{ var h = Swift.Hasher(); hash(into: &h); return h.finalize() }}')
                elif overridden in ('#rawValue', '#allCases') and kw == 'get':
                    info = enums[top[3]]
                    if overridden == '#rawValue':
                        arms = ' '.join(f'case .{c}: return {v}' for c, v in zip(info['cases'], info['values']))
                        out.append(f'{indent}{mods}get {{ switch self {{ {arms} }} }}')
                    else:
                        out.append(f'{indent}{mods}get {{ return [{", ".join("." + c for c in info["cases"])}] }}')
                elif overridden and kw == 'get':
                    out.append(f'{indent}{mods}{kw} {asy}{thr}{{ return super.{overridden} }}')
                elif overridden and kw == 'set':
                    out.append(f'{indent}{mods}{kw} {{ super.{overridden} = newValue }}')
                else:
                    out.append(f'{indent}{mods}{kw} {asy}{thr}{stub_body(kw, ty)}')
                continue
            # An inlinable accessor's body, or attributes before an accessor.
            out.append(line)
            d = braces(line)
            stack.extend([('body',)] * max(d, 0))
            continue

        if kind == 'pvar':
            out.append(line)
            continue

        d = braces(line)
        code = strip_strings(stripped)

        # Attributes on their own lines belong to the next declaration.
        if re.match(r'^(@[\w.]+(\((?:[^()]|\([^()]*\))*\))?\s*)+$', stripped) and kind != 'protocol':
            pending.append(len(out))
            out.append(line)
            continue
        attrs = ' '.join(out[i].strip() for i in pending) + ' ' + stripped
        frozen_pending = '@frozen' in attrs
        missing_inits = '@_hasMissingDesignatedInitializers' in attrs
        attr_lines = pending
        pending = []
        ext = re.match(r'^extension\s+(?:\w+)\.(\w+)', code)
        named = re.search(r'\b(?:struct|class|enum|protocol|actor|typealias)\s+(\w+)', code)
        if (IOS_UNAVAILABLE.search(attrs) or (ext and ext.group(1) in dropped)) and kind != 'protocol' \
                and stripped and not stripped.startswith('}'):
            # Not for iOS apps (or an extension of what isn't): left out, with its attributes.
            if named and kind == 'file':
                dropped.add(named.group(1))
            for i in reversed(attr_lines):
                del out[i]
            if d > 0:
                stack.extend([('skip',)] * d)
            continue

        # A member pomme implements, or an owned type's deinit: left out.
        if kind == 'type' and stripped and not stripped.startswith('}'):
            k = (top[1], key(stripped))
            SEEN.append(f'// pomme: replaces {k[0]} :: {k[1]}')
            if k in replaces or (top[2] and decl_keyword(line) == 'deinit'):
                used.add(k)
                for i in reversed(attr_lines):
                    del out[i]
                if d > 0:
                    stack.extend([('skip',)] * d)
                continue

        # A property wrapper's property (`@Binding @_projectedValueProperty($x)
        # public var x: T { … }`): declared with the wrapper, which makes `x`,
        # `$x` and the storage, as Apple's source does.
        m = re.search(r'@_projectedValueProperty\((\$\w+)\)\s*', line)
        if m and kind == 'type':
            projected.add(m.group(1))
            decl = line.replace(m.group(0), '')
            decl = re.sub(r'\s*\{\s*$', '', decl)
            out.append(decl)
            if d > 0:
                stack.extend([('skip',)] * d)
            continue
        m = re.search(r'\bvar\s+(\$\w+)\s*:', code)
        if m and m.group(1) in projected and kind == 'type':
            stack.extend([('skip',)] * max(d, 0))
            continue

        kw = decl_keyword(line)
        if kw:
            sig, rest = split_at_body(line)
            if rest is None and kind != 'protocol':
                if kw == 'deinit':
                    out.append(line + ' {}')
                elif kind == 'type' and (info := enums.get(top[4])) and not info['payload'] and info['cases'] \
                        and (body := derived_body(key(stripped), info['cases'], info)):
                    out.append(f'{line} {body}')
                elif kind == 'type' and re.fullmatch(r'public init\(rawValue: [\w.]+\)', key(stripped)):
                    # A raw-value type (an option set, a string-backed name …): its raw value is all it holds.
                    store = '_pommeStorage = (); ' if top[3] else ''
                    out.append(f'{line} {{ {store}self.rawValue = rawValue }}')
                elif kind == 'type' and kw == 'func' and re.search(r'\boverride\b', MODIFIERS.match(ATTRIBUTES.sub('', stripped)).group(0)) \
                        and (call := super_call(sig)):
                    # Overriding: the superclass's behavior, until pomme has its own.
                    out.append(f'{nonisolated_if_concurrent(line, sig, kind)} {{ return {call} }}')
                else:
                    out.append(f'{nonisolated_if_concurrent(line, sig, kind)} {stub_body(stripped, result_type(sig))}')
                continue
            out.append(nonisolated_if_concurrent(line, sig, kind) if rest is not None else line)
            if kw == 'subscript' and rest is not None and rest.strip() == '{':
                # A subscript's accessor list, as a property's.
                stack.append(('pvar',) if kind == 'protocol' else ('var', result_type(sig)))
            elif rest is not None:
                # An inlinable body (on this line, or opening here).
                stack.extend([('body',)] * max(braces(rest), 0))
            continue

        if d > 0 and code.endswith('{'):
            head = code[:-1]
            if TYPE_OPEN.search(head) and not re.search(r'\b(var|let)\b', head):
                hk = key(stripped)
                if kind == 'type' and not top[1].startswith('extension '):
                    # Nested in a type's declaration: named with it (`Published<Value> > Publisher …`).
                    hk = f'{top[1]} > {hk}'
                owned = hk in owns
                if kind == 'file' and not re.search(r'^extension\b', hk):
                    SEEN.append(f'// pomme: owns {hk}')
                # ('type', key, owned, whether it gets hidden storage, qualified name)
                stack.append(('protocol',) if re.search(r'\bprotocol\b', head) else ('type', hk, owned, m_storage_needed(head, frozen_pending), qualified_name(head, top, modules)))
                stack.extend([('body',)] * (d - 1))
                if hk in inside and not owned and hk not in used:
                    # pomme's members of the declaration itself, first.
                    used.add(hk)
                    out.append(line)
                    out.append(f'  // pomme ({inside[hk][1]}):')
                    out.extend(inside[hk][0])
                    if m_storage_needed(head, frozen_pending):
                        out.append('  internal let _pommeStorage: ()')
                    continue
                if owned:
                    # pomme declares the type (its storage, conformances and
                    # attributes); the interface's members extend it.
                    used.add(hk)
                    m = re.search(r'\b(?:struct|class|enum|actor)\s+(\w+)', head)
                    assert m and kind == 'file', f'only top-level types can be owned: {hk}'
                    for i in reversed(attr_lines):
                        del out[i]
                    out.append(f'extension {m.group(1)} {{')
                    continue
                m = re.search(r'\b(struct|class|enum|actor)\s+(\w+)', head)
                if m and m.group(2) in STORAGE:
                    out.append(line)
                    out.extend('  ' + member for member in STORAGE[m.group(2)])
                    continue
                if m and m.group(1) == 'class' and missing_inits:
                    # Its designated initializers aren't in the interface: one that stops.
                    out.append(line)
                    # (Labelled per class, so a subclass's doesn't override its superclass's.)
                    out.append(f'  internal init(_pomme{m.group(2)}: ()) {{ _pommeUnimplemented("an initializer the interface doesn\'t show") }}')
                    continue
                if m and m.group(1) == 'struct' and not re.search(r'@frozen\b', head) and not frozen_pending:
                    # A resilient struct has storage the interface doesn't show.
                    # Without any, Swift would synthesize an init() that clashes
                    # with the struct's own. (Its layout is private to SwiftUI.)
                    out.append(line)
                    out.append('  internal let _pommeStorage: ()')
                    continue
            elif re.search(r'\b(var|let)\b', head) and d == 1:
                # (An overriding property's accessors are the superclass's, until pomme has its own.)
                name = re.search(r'\b(?:var|let)\s+([`\w]+)', head).group(1)
                overriding = kind == 'type' and re.search(r'\boverride\b', MODIFIERS.match(ATTRIBUTES.sub('', stripped)).group(0))
                info = enums.get(top[4]) if kind == 'type' else None
                plain = info and not info['payload'] and info['cases']
                derived = plain and name in ('hashValue', 'allCases') or plain and name == 'rawValue' and info['raw']
                # An environment value (`EnvironmentValues.layoutDirection` …): kept by name.
                environment = kind == 'type' and top[4] == 'EnvironmentValues' and top[1].startswith('extension') \
                    and not re.search(r'\bstatic\b', head)
                marker = f'#{name}' if derived else f'#env:{name}' if environment else name if overriding else None
                stack.append(('pvar',) if kind == 'protocol' else ('var', property_type(head), marker, top[4] if kind == 'type' else None))
            else:
                stack.extend([('body',)] * d)
            out.append(line)
            continue

        if d < 0:
            out.append(line)
            for _ in range(-d):
                if stack:
                    stack.pop()
            continue

        # An enum's cases, for its derived members (see derived_body).
        if kind == 'type' and re.match(r'(indirect\s+)?case\s', code) and re.search(r'\benum\s', top[1]):
            raw = re.search(r'\benum\s+\w+(?:<[^>]*>)?\s*:\s*(Swift\.(?:U?Int\d*|String|Double|Float))\b', top[1])
            info = enums.setdefault(top[4], {'cases': [], 'payload': False, 'raw': raw and raw.group(1), 'values': []})
            for c in split_top(re.sub(r'^(indirect\s+)?case\s+', '', code)):
                info['payload'] |= '(' in c
                name = re.match(r'\s*`?(\w+)', c).group(1)
                explicit = re.search(r'=\s*(.+)$', c)
                if explicit:
                    value = explicit.group(1).strip()
                elif info['raw'] == 'Swift.String':
                    value = f'"{name}"'
                else:
                    value = str(int(info['values'][-1]) + 1) if info['values'] else '0'
                info['cases'].append(name)
                info['values'].append(value)

        # An option set's options: bits in the order they're declared (as Apple
        # declares them); `none`, `all`, `horizontal` and `vertical` combine them.
        m = re.search(r'\bstatic\s+let\s+`?(\w+)`?\s*:\s*([\w.]+)\s*$', code)
        if kind == 'type' and m and 'Swift.OptionSet' in top[1] and re.search(r'\b(struct|class)\s+' + re.escape(m.group(2).split('.')[-1]) + r'\b', top[1]):
            options = option_bits.setdefault(top[1], {})
            name = m.group(1)
            parts = {'horizontal': ('leading', 'trailing', 'left', 'right'), 'vertical': ('top', 'bottom', 'up', 'down')}
            if name == 'none':
                value = 0
            elif name == 'all':
                value = 0
                for v in options.values():
                    value |= v
            elif name in parts and any(p in options for p in parts[name]):
                value = 0
                for p in parts[name]:
                    value |= options.get(p, 0)
            elif len(named := [v for k, v in options.items() if name in camel_words(k)]) >= 2:
                # Named by a word of earlier options' names: those together (`leading`: `topLeading`, `bottomLeading`).
                value = 0
                for v in named:
                    value |= v
            else:
                singles = [v for v in options.values() if v and v & (v - 1) == 0]
                value = 1 << len(singles)
            options[name] = value
            out.append(f'{line} = .init(rawValue: {value})')
            continue

        # A static or global stored property needs a value: using it stops.
        if (kind == 'file' or re.search(r'\bstatic\s+(let|var)\b', code)) and re.search(r'\b(let|var)\s+[`\w]+\s*:', code) \
                and '=' not in code and '{' not in code and kind != 'protocol':
            what = re.sub(r'\s+', ' ', stripped)[:80].replace('"', '\\"')
            out.append(f'{line} = _pommeUnimplementedValue("{what}")')
            continue

        out.append(line)
    unused = [w for k, w in list(owns.items()) + list(replaces.items()) if k not in used]
    unused += [w for k, (_, w) in inside.items() if k not in used]
    if unused:
        sys.exit('stubgen: markers that match no declaration (has the SDK changed?):\n  ' + '\n  '.join(unused))
    return '\n'.join(out)


# The stops. SwiftUI's two modules mangle as SwiftUI and share a package
# (SwiftUI), so they're defined once, in SwiftUICore, with `package` access;
# any other module has its own.
# package (SwiftUI), so SwiftUI uses these through `package` access.
SUPPORT = '''
// ---- pomme ----
/// An API pomme's {framework} doesn't implement yet: stop, naming it.
{access} func _pommeUnimplemented(_ what: Swift.StaticString, file: Swift.StaticString = #fileID, line: Swift.UInt = #line) -> Swift.Never {{
  Swift.fatalError("{framework} in pomme doesn't implement \\(what) yet", file: file, line: line)
}}
{access} func _pommeUnimplementedValue<T>(_ what: Swift.StaticString) -> T {{
  Swift.fatalError("{framework} in pomme doesn't implement \\(what) yet")
}}
'''

if __name__ == '__main__' and sys.argv[1] == '--markers':
    # stubgen.py --markers <interface> <regex>: the markers for matching declarations.
    generate(open(sys.argv[2]).read())
    print('\n'.join(m for m in SEEN if re.search(sys.argv[3], m)))
elif __name__ == '__main__':
    # stubgen.py <interface> <output.swift> <processed dir> [pomme's sources…]
    text = open(sys.argv[1]).read()
    owns, replaces, inside = read_markers(sys.argv[4:], sys.argv[3])
    body = generate(text, owns, replaces, inside)
    body = '\n'.join(l for l in body.split('\n') if not l.startswith('// swift-'))
    module = re.search(r'-module-name (\w+)', text.split('\n', 5)[2]).group(1)
    support = {'SwiftUICore': SUPPORT.format(framework='SwiftUI', access='package'), 'SwiftUI': ''}
    open(sys.argv[2], 'w').write(body + '\n' + support.get(module, SUPPORT.format(framework=module, access='internal')))
