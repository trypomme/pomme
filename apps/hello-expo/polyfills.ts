// Globals MSW expects that React Native doesn't provide. Their own module,
// imported first, so they exist before MSW's modules evaluate.

// MessageEvent: React Native has one but doesn't expose it (@msw/react-native's
// polyfills do the same).
// @ts-expect-error: React Native's private web API module has no types.
import RNMessageEvent from 'react-native/src/private/webapis/html/events/MessageEvent';

(globalThis as any).MessageEvent ??= RNMessageEvent;

// Promise.withResolvers (ES2024): Hermes doesn't have it yet.
if (!(Promise as any).withResolvers) {
  (Promise as any).withResolvers = function <T>() {
    let resolve!: (v: T | PromiseLike<T>) => void;
    let reject!: (e?: unknown) => void;
    const promise = new Promise<T>((res, rej) => {
      resolve = res;
      reject = rej;
    });
    return { promise, resolve, reject };
  };
}

// Headers.getSetCookie (2023): React Native's Headers predates it; MSW calls it.
if (typeof Headers !== 'undefined' && !(Headers.prototype as any).getSetCookie) {
  (Headers.prototype as any).getSetCookie = function (this: Headers) {
    const v = this.get('set-cookie');
    return v ? v.split(/,(?=\s*[^;=]+=)/).map((c) => c.trim()) : [];
  };
}

// BroadcastChannel: there's one JS context, so channels of the same name just
// deliver to each other, asynchronously as on the web.
if (!(globalThis as any).BroadcastChannel) {
  const channels = new Map<string, Set<any>>();
  class BroadcastChannel {
    readonly name: string;
    onmessage: ((event: { data: unknown }) => void) | null = null;
    private listeners = new Set<(event: { data: unknown }) => void>();
    private closed = false;
    constructor(name: string) {
      this.name = String(name);
      if (!channels.has(this.name)) channels.set(this.name, new Set());
      channels.get(this.name)!.add(this);
    }
    postMessage(data: unknown) {
      if (this.closed) throw new Error('BroadcastChannel is closed');
      for (const other of channels.get(this.name) ?? []) {
        if (other !== this) setTimeout(() => other.deliver(structuredClone(data)), 0);
      }
    }
    private deliver(data: unknown) {
      if (this.closed) return;
      const event = { data, target: this };
      this.onmessage?.(event);
      for (const l of this.listeners) l(event);
    }
    addEventListener(type: string, listener: (event: { data: unknown }) => void) {
      if (type === 'message') this.listeners.add(listener);
    }
    removeEventListener(type: string, listener: (event: { data: unknown }) => void) {
      if (type === 'message') this.listeners.delete(listener);
    }
    close() {
      this.closed = true;
      channels.get(this.name)?.delete(this);
    }
  }
  (globalThis as any).BroadcastChannel = BroadcastChannel;
}

// Log errors with their stacks (release builds otherwise print just the message).
const consoleError = console.error;
console.error = (...args: unknown[]) =>
  consoleError(...args.map((a) => (a instanceof Error && a.stack ? a.stack : a)));
