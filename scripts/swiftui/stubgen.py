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

Usage: stubgen.py <interface> <output.swift>
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


def generate(src):
    out = []
    # Context stack entries: ('protocol',), ('type',), ('var', result type),
    # ('pvar',) for a protocol's property requirements, ('body',) for code.
    stack = []
    projected = set()  # wrapped properties' projections, declared by their wrappers
    pending = []  # indexes in `out` of attribute lines waiting for their declaration
    frozen_pending = False
    missing_inits = False
    dropped = set()  # top-level types left out (unavailable on iOS)
    for raw in src.split('\n'):
        line = raw.rstrip()
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
        ext = re.match(r'^extension\s+(?:Swift|SwiftUICore|SwiftUI)\.(\w+)', code)
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
                    out.append(f'{line} {{}}')
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
                stack.append(('protocol',) if re.search(r'\bprotocol\b', head) else ('type',))
                stack.extend([('body',)] * (d - 1))
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
                stack.append(('pvar',) if kind == 'protocol' else ('var', property_type(head)))
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

        # A static or global stored property needs a value: using it stops.
        if (kind == 'file' or re.search(r'\bstatic\s+(let|var)\b', code)) and re.search(r'\b(let|var)\s+[`\w]+\s*:', code) \
                and '=' not in code and '{' not in code and kind != 'protocol':
            what = re.sub(r'\s+', ' ', stripped)[:80].replace('"', '\\"')
            out.append(f'{line} = _pommeUnimplementedValue("{what}")')
            continue

        out.append(line)
    return '\n'.join(out)


# Defined once, in SwiftUICore: both modules mangle as SwiftUI, and share the
# package (SwiftUI), so SwiftUI uses these through `package` access.
SUPPORT = '''
// ---- pomme ----
/// An API pomme's SwiftUI doesn't implement yet: stop, naming it.
package func _pommeUnimplemented(_ what: Swift.StaticString, file: Swift.StaticString = #fileID, line: Swift.UInt = #line) -> Swift.Never {
  Swift.fatalError("SwiftUI in pomme doesn't implement \\(what) yet", file: file, line: line)
}
package func _pommeUnimplementedValue<T>(_ what: Swift.StaticString) -> T {
  Swift.fatalError("SwiftUI in pomme doesn't implement \\(what) yet")
}
'''

if __name__ == '__main__':
    text = open(sys.argv[1]).read()
    body = generate(text)
    body = '\n'.join(l for l in body.split('\n') if not l.startswith('// swift-'))
    core = '-module-name SwiftUICore' in text.split('\n', 5)[2]
    open(sys.argv[2], 'w').write(body + '\n' + (SUPPORT if core else ''))
