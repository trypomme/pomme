import './polyfills';

// Mock Service Worker for React Native, set up the way @msw/react-native does
// (fetch and XMLHttpRequest interceptors under MSW's network API). Only
// https://pomme.test is mocked; every other request goes to the network.
import { FetchInterceptor } from '@mswjs/interceptors/fetch/web';
import { XMLHttpRequestInterceptor } from '@mswjs/interceptors/XMLHttpRequest/web';
import { defineNetwork, InterceptorSource } from 'msw/experimental';
import { http, HttpResponse } from 'msw';

// React Native's own fetch, before MSW wraps it. Requests MSW passes through
// lose their response bodies on iOS (a known MSW + React Native issue; real
// devices and pomme agree), so the app's real data fetching uses this.
export const nativeFetch: typeof fetch = globalThis.fetch.bind(globalThis);

export const network = defineNetwork({
  sources: [
    new InterceptorSource({
      // msw's bundled types inline their own copy of Interceptor, so the classes
      // don't type-check against it (a known MSW issue); at runtime they're the same.
      interceptors: [new FetchInterceptor(), new XMLHttpRequestInterceptor()] as any,
    }),
  ],
  onUnhandledFrame: 'bypass',
  context: { quiet: true },
});

network.use(
  http.get('https://pomme.test/hello', () => HttpResponse.json({ greeting: 'hello from MSW' })),
);

network.enable();
