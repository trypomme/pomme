import { useEffect, useState } from 'react';
import { StatusBar } from 'expo-status-bar';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import useSWR from 'swr';
import { QueryClient, QueryClientProvider, useQuery } from '@tanstack/react-query';

// pomme's test app: a tap counter, and data fetching three ways (SWR,
// TanStack Query, plain Promise.all) against the GitHub API.

type Repo = { full_name: string; stargazers_count: number };

const API = 'https://api.github.com/repos/';

const fetchRepo = async (url: string): Promise<Repo> => {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
};

// Thousands separators without Intl (keeps the app independent of locale data).
const group = (n: number) => String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ',');

const describe = (data?: Repo, error?: Error | null) =>
  error ? `Couldn't load: ${error.message}` : data ? `${data.full_name} ★ ${group(data.stargazers_count)}` : 'Loading…';

function SwrStars() {
  const { data, error, isValidating, mutate } = useSWR(`${API}vercel/swr`, fetchRepo);
  return (
    <View style={styles.card}>
      <Text style={styles.cardTitle}>SWR</Text>
      <Text testID="swr-status" style={styles.cardText}>{describe(data, error)}</Text>
      <Pressable testID="swr-refresh" onPress={() => mutate()} style={styles.refresh}>
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
      <Pressable testID="query-refresh" onPress={() => refetch()} style={styles.refresh}>
        <Text style={styles.refreshLabel}>{isFetching ? 'Refreshing…' : 'Refresh'}</Text>
      </Pressable>
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

const queryClient = new QueryClient();

export default function App() {
  const [taps, setTaps] = useState(0);
  return (
    <QueryClientProvider client={queryClient}>
      <View style={styles.container}>
        <Text>Hello from pomme 🍎</Text>
        <Pressable
          testID="tap-button"
          onPress={() => setTaps((n) => n + 1)}
          style={({ pressed }) => [styles.button, pressed && styles.pressed]}
        >
          <Text style={styles.label}>Tapped {taps} {taps === 1 ? 'time' : 'times'}</Text>
        </Pressable>
        <SwrStars />
        <QueryStars />
        <AllStars />
        <StatusBar style="auto" />
      </View>
    </QueryClientProvider>
  );
}

const styles = StyleSheet.create({
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
});
