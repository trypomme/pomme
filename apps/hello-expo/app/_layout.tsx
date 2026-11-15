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
        <Stack.Screen name="gestures" options={{ title: 'Gestures' }} />
        <Stack.Screen name="sticky" options={{ title: 'Sticky' }} />
        <Stack.Screen name="rotation" options={{ title: 'Rotation' }} />
        <Stack.Screen name="image" options={{ title: 'Image' }} />
        <Stack.Screen name="network" options={{ title: 'Network' }} />
        <Stack.Screen name="storage" options={{ title: 'Storage' }} />
        <Stack.Screen name="lock" options={{ title: 'Lock' }} />
        <Stack.Screen name="firebase" options={{ title: 'Firebase' }} />
        <Stack.Screen name="tabs" options={{ headerShown: false }} />
        <Stack.Screen name="glass" options={{ title: 'Glass' }} />
        <Stack.Screen name="moretabs" options={{ headerShown: false }} />
      </Stack>
    </QueryClientProvider>
  );
}
