import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Stack } from 'expo-router';

const queryClient = new QueryClient();

export default function RootLayout() {
  return (
    <QueryClientProvider client={queryClient}>
      <Stack>
        <Stack.Screen name="index" options={{ title: 'pomme' }} />
        <Stack.Screen name="repo/[owner]/[name]" options={{ title: 'Repository' }} />
        <Stack.Screen name="keyboard" options={{ title: 'Keyboard' }} />
        <Stack.Screen name="notes" options={{ title: 'Notes' }} />
        <Stack.Screen name="list" options={{ title: 'List' }} />
      </Stack>
    </QueryClientProvider>
  );
}
