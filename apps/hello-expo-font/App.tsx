// expo-font on pomme: a font embedded at build time (Roboto Slab, by the
// config plugin in app.json), a font loaded at run time (Roboto Medium, with
// useFonts), weights of a variable font, what the module reports
// (isLoaded, getLoadedFonts), and text rendered to an image
// (renderToImageAsync).

import { getLoadedFonts, isLoaded, renderToImageAsync, useFonts } from 'expo-font';
import type { RenderToImageResult } from 'expo-font';
import { StatusBar } from 'expo-status-bar';
import { useEffect, useState } from 'react';
import { Image, SafeAreaView, StyleSheet, Text, View } from 'react-native';

export default function App() {
  const [loaded, error] = useFonts({
    'Roboto-Medium': require('./assets/fonts/Roboto-Medium.ttf'),
  });
  const [image, setImage] = useState<RenderToImageResult | null>(null);
  const [imageError, setImageError] = useState<string | null>(null);

  useEffect(() => {
    if (!loaded) return;
    renderToImageAsync('Rendered', { fontFamily: 'Roboto Slab', size: 32, color: '#c2410c' }).then(setImage, (e) =>
      setImageError(String(e)),
    );
  }, [loaded]);

  if (!loaded && !error) {
    return null;
  }

  return (
    <SafeAreaView style={styles.page}>
      <View style={styles.content}>
        <Text style={styles.system}>System font</Text>
        <Text style={styles.slab}>Embedded: Roboto Slab</Text>
        <Text style={[styles.slab, styles.heavy]}>Roboto Slab 800</Text>
        <Text style={[styles.slab, styles.light]}>Roboto Slab 200</Text>
        <Text style={styles.runtime}>Loaded: Roboto Medium</Text>
        <Text style={styles.status}>
          {error ? `error ${error.message}` : `loaded ${loaded} · isLoaded ${isLoaded('Roboto-Medium')}`}
        </Text>
        <Text style={styles.status}>{`fonts ${getLoadedFonts().sort().join(', ')}`}</Text>
        {image && <Image source={{ uri: image.uri }} style={{ width: image.width, height: image.height }} />}
        {image && <Text style={styles.status}>{`image ${Math.round(image.width)}×${Math.round(image.height)} @${image.scale}x`}</Text>}
        {imageError && <Text style={styles.status}>{`image error ${imageError}`}</Text>}
      </View>
      <StatusBar style="auto" />
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1, backgroundColor: '#fff' },
  content: { padding: 24, gap: 12 },
  system: { fontSize: 28 },
  slab: { fontFamily: 'Roboto Slab', fontSize: 28 },
  heavy: { fontWeight: '800' },
  light: { fontWeight: '200' },
  runtime: { fontFamily: 'Roboto-Medium', fontSize: 28 },
  status: { fontSize: 14, color: '#555' },
});
