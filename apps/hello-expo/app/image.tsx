import { Image } from 'expo-image';
import { useState } from 'react';
import { ScrollView, StyleSheet, Text } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

// expo-image: a bundled PNG (four coloured quadrants), a remote JPEG (three
// coloured thirds, 300 × 100) shown with contentFit cover and contain, and a
// blurhash placeholder for an image that never loads. The status line shows
// what onLoad / onError reported. Reached with the pommehello:///image link.

const HEADER_HEIGHT = 44;
const PHOTO = 'https://images.pomme.test/wide.jpg';
const BLURHASH = 'LEHV6nWB2yk8pyo0adR*.7kCMdnj';

export default function ImageScreen() {
  const top = useSafeAreaInsets().top + HEADER_HEIGHT;
  const [loads, setLoads] = useState<string[]>([]);
  const loaded = (name: string) => (e: { source: { width: number; height: number } }) =>
    setLoads((l) => [...l, `${name} ${e.source.width}x${e.source.height}`].sort());
  return (
    <ScrollView style={styles.page} contentContainerStyle={{ paddingTop: top }}>
      <Text testID="status" style={styles.status}>
        {loads.length ? loads.join(' · ') : 'loading'}
      </Text>
      <Image testID="local" source={require('../assets/images/quadrants.png')} style={styles.square} onLoad={loaded('local')} />
      <Image testID="cover" source={PHOTO} contentFit="cover" style={styles.square} onLoad={loaded('cover')} />
      <Image testID="contain" source={PHOTO} contentFit="contain" style={styles.square} onLoad={loaded('contain')} />
      <Image testID="blurhash" source="https://images.pomme.test/missing.jpg" placeholder={{ blurhash: BLURHASH }} style={styles.square} />
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1, backgroundColor: '#fff' },
  status: { fontSize: 15, padding: 12 },
  square: { width: 120, height: 120, marginLeft: 20, marginBottom: 12, backgroundColor: '#e5e7eb' },
});
