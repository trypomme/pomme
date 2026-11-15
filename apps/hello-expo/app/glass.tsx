import { GlassContainer, GlassView, isLiquidGlassAvailable } from 'expo-glass-effect';
import { StyleSheet, Text, View } from 'react-native';

// Liquid Glass through Expo's expo-glass-effect (iOS 26's UIGlassEffect):
// regular, clear and tinted glass over stripes, and two in a container.
// Reached with the pommehello:///glass link.

const STRIPES = ['#000000', '#ff9500', '#000000', '#ff9500', '#000000', '#ff9500', '#000000'];

export default function GlassScreen() {
  return (
    <View style={styles.page}>
      <View style={styles.stripes}>
        {STRIPES.map((c, i) => (
          <View key={i} style={[styles.stripe, { backgroundColor: c }]} />
        ))}
      </View>
      <Text testID="available" style={styles.line}>
        Liquid Glass {isLiquidGlassAvailable() ? 'available' : 'unavailable'}
      </Text>
      <GlassView testID="regular" style={styles.glass} glassEffectStyle="regular">
        <Text style={styles.label}>Regular</Text>
      </GlassView>
      <GlassView testID="clear" style={styles.glass} glassEffectStyle="clear">
        <Text style={styles.label}>Clear</Text>
      </GlassView>
      <GlassView testID="tinted" style={styles.glass} glassEffectStyle="regular" tintColor="#007aff" isInteractive>
        <Text style={[styles.label, { color: '#fff' }]}>Tinted</Text>
      </GlassView>
      <GlassContainer spacing={20} style={styles.row}>
        <GlassView style={styles.circle} />
        <GlassView style={styles.circle} />
      </GlassContainer>
    </View>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1, backgroundColor: '#fff', paddingTop: 120, paddingHorizontal: 20, gap: 20 },
  stripes: { position: 'absolute', top: 160, left: 20, flexDirection: 'row' },
  stripe: { width: 25, height: 480 },
  line: { fontSize: 17 },
  glass: { height: 80, borderRadius: 24, justifyContent: 'center', paddingHorizontal: 24 },
  label: { fontSize: 20, fontWeight: '600' },
  row: { flexDirection: 'row', gap: 20 },
  circle: { width: 80, height: 80, borderRadius: 40 },
});
