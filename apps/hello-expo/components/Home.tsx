import { nativeFetch } from '../mocks';
import { useEffect, useState } from 'react';
import { StatusBar } from 'expo-status-bar';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import useSWR from 'swr';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'expo-router';
import { CodedError, EventEmitter, Platform, requireOptionalNativeModule, uuid } from 'expo';
import { fetch as expoFetch } from 'expo/fetch';

// The home screen: a tap counter, data fetching three ways (SWR, TanStack
// Query, plain Promise.all) against the GitHub API, a request mocked with MSW,
// and checks of the `expo` package's APIs.

export type Repo = { full_name: string; stargazers_count: number };

export const API = 'https://api.github.com/repos/';

export const fetchRepo = async (url: string): Promise<Repo> => {
  const res = await nativeFetch(url);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
};

// Thousands separators without Intl (keeps the app independent of locale data).
export const group = (n: number) => String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ',');

const describe = (data?: Repo, error?: Error | null) =>
  error ? `Couldn't load: ${error.message}` : data ? `${data.full_name} ★ ${group(data.stargazers_count)}` : 'Loading…';

function SwrStars() {
  const { data, error, isValidating, mutate } = useSWR(`${API}vercel/swr`, fetchRepo);
  return (
    <View style={styles.card}>
      <Text style={styles.cardTitle}>SWR</Text>
      <Text testID="swr-status" style={styles.cardText}>{describe(data, error)}</Text>
      <Pressable testID="swr-refresh" role="button" onPress={() => mutate()} style={styles.refresh}>
        <Text style={styles.refreshLabel}>{isValidating ? 'Refreshing…' : 'Refresh'}</Text>
      </Pressable>
    </View>
  );
}

function QueryStars() {
  const { data, error, isFetching, refetch } = useQuery({
    queryKey: ['repo', 'TanStack/query'],
    queryFn: () => fetchRepo(`${API}TanStack/query`),
  });
  return (
    <View style={styles.card}>
      <Text style={styles.cardTitle}>TanStack Query</Text>
      <Text testID="query-status" style={styles.cardText}>{describe(data, error)}</Text>
      <View style={styles.row}>
        <Pressable testID="query-refresh" role="button" onPress={() => refetch()} style={styles.refresh}>
          <Text style={styles.refreshLabel}>{isFetching ? 'Refreshing…' : 'Refresh'}</Text>
        </Pressable>
        <Link testID="query-details" href="/repo/TanStack/query" style={styles.refresh}>
          <Text style={styles.refreshLabel}>Details</Text>
        </Link>
      </View>
    </View>
  );
}

function AllStars() {
  const [text, setText] = useState('Promise.all: loading…');
  useEffect(() => {
    Promise.all(['vercel/swr', 'TanStack/query', 'facebook/react'].map((r) => fetchRepo(API + r)))
      .then((repos) => {
        const total = repos.reduce((n, r) => n + r.stargazers_count, 0);
        setText(`Promise.all: ${repos.length} repos, ★ ${group(total)}`);
      })
      .catch((e) => setText(`Promise.all failed: ${e?.message ?? e}`));
  }, []);
  return <Text testID="all-status">{text}</Text>;
}

// The `expo` package's web-standard and core APIs, one check each.
const checks: [string, () => boolean | Promise<boolean>][] = [
  ['TextEncoder', () => {
    const bytes = new TextEncoder().encode('pomme 🍎');
    return bytes.length === 10 && new TextDecoder().decode(bytes) === 'pomme 🍎';
  }],
  ['URL', () => {
    const u = new URL('https://example.com/a/b?x=1&y=two#h');
    return u.pathname === '/a/b' && u.searchParams.get('y') === 'two' && u.hash === '#h';
  }],
  ['URLSearchParams', () => new URLSearchParams({ a: '1', b: 'x y' }).toString() === 'a=1&b=x+y'],
  ['structuredClone', () => {
    const a = { m: new Map([[1, { deep: [1, 2] }]]), d: new Date(0) };
    const b = structuredClone(a);
    return b !== a && b.m.get(1)?.deep[1] === 2 && b.m.get(1) !== a.m.get(1) && b.d.getTime() === 0;
  }],
  ['ReadableStream', async () => {
    const stream = new ReadableStream<string>({
      start(c) { c.enqueue('po'); c.enqueue('mme'); c.close(); },
    });
    const reader = stream.getReader();
    let out = '';
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      out += value;
    }
    return out === 'pomme';
  }],
  ['EventEmitter', () => {
    const e = new EventEmitter<{ ping: (n: number) => void }>();
    let got = 0;
    const sub = e.addListener('ping', (n) => { got += n; });
    e.emit('ping', 2);
    sub.remove();
    e.emit('ping', 5);
    return got === 2;
  }],
  ['CodedError', () => new CodedError('ERR_POMME', 'x').code === 'ERR_POMME'],
  ['Platform', () => Platform.OS === 'ios'],
  ['uuid', () => /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(uuid.v4())],
  ['native module', () => requireOptionalNativeModule('ExpoFetchModule') != null],
  ['expo/fetch stream', async () => {
    const res = await expoFetch('https://api.github.com/repos/vercel/swr');
    const reader = res.body!.getReader();
    let n = 0;
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      n += value.length;
    }
    return res.ok && n > 0;
  }],
];

function SdkChecks() {
  const [text, setText] = useState('Expo SDK: checking…');
  useEffect(() => {
    (async () => {
      const failed: string[] = [];
      for (const [name, check] of checks) {
        try {
          if (!(await check())) failed.push(name);
        } catch (e: any) {
          failed.push(`${name} (${e?.message ?? e})`);
        }
      }
      const passed = checks.length - failed.length;
      setText(`Expo SDK: ${passed}/${checks.length} passed${failed.length ? ` — failed: ${failed.join(', ')}` : ''}`);
    })();
  }, []);
  return <Text testID="sdk-status" style={styles.small}>{text}</Text>;
}

function MswGreeting() {
  const [text, setText] = useState('MSW: loading…');
  useEffect(() => {
    fetch('https://pomme.test/hello')
      .then((r) => r.json())
      .then((j) => setText(`MSW: ${j.greeting}`))
      .catch((e) => setText(`MSW failed: ${e?.message ?? e}`));
  }, []);
  return <Text testID="msw-status" style={styles.small}>{text}</Text>;
}

export default function Home() {
  const [taps, setTaps] = useState(0);
  return (
    <View style={styles.container}>
      <Text>Hello from pomme 🍎</Text>
      <Pressable
        testID="tap-button"
        role="button"
        onPress={() => setTaps((n) => n + 1)}
        style={({ pressed }) => [styles.button, pressed && styles.pressed]}
      >
        <Text style={styles.label}>Tapped {taps} {taps === 1 ? 'time' : 'times'}</Text>
      </Pressable>
      <SwrStars />
      <QueryStars />
      <AllStars />
      <MswGreeting />
      <SdkChecks />
      <StatusBar style="auto" />
    </View>
  );
}

export const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#fff',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 16,
  },
  button: {
    backgroundColor: '#e8423b',
    borderRadius: 12,
    paddingHorizontal: 20,
    paddingVertical: 12,
  },
  pressed: {
    opacity: 0.6,
  },
  label: {
    color: '#fff',
    fontSize: 17,
    fontWeight: '600',
  },
  card: {
    width: 300,
    alignItems: 'center',
    gap: 8,
    padding: 14,
    borderRadius: 12,
    borderWidth: 1,
    borderColor: '#d1d1d6',
  },
  cardTitle: {
    fontSize: 13,
    fontWeight: '600',
    color: '#6e6e73',
  },
  cardText: {
    fontSize: 15,
  },
  refresh: {
    paddingHorizontal: 14,
    paddingVertical: 6,
    borderRadius: 8,
    backgroundColor: '#f2f2f7',
  },
  refreshLabel: {
    color: '#007aff',
    fontSize: 15,
  },
  row: {
    flexDirection: 'row',
    gap: 8,
  },
  small: {
    fontSize: 13,
    color: '#3a3a3c',
    maxWidth: 340,
    textAlign: 'center',
  },
});
