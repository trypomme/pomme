import { useEffect, useState } from 'react';
import { Dimensions, StyleSheet, Text, View, useWindowDimensions } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

// Turning the phone: the window's size (useWindowDimensions), how many times
// React Native reported a new size (Dimensions 'change'), and the safe-area
// insets (react-native-safe-area-context). A bar spans the safe area's width.
// Reached with the pommehello:///rotation link.

const HEADER_HEIGHT = 44;

export default function RotationScreen() {
  const { width, height } = useWindowDimensions();
  const insets = useSafeAreaInsets();
  const [changes, setChanges] = useState(0);
  useEffect(() => {
    const sub = Dimensions.addEventListener('change', () => setChanges((n) => n + 1));
    return () => sub.remove();
  }, []);
  const landscape = width > height;
  return (
    <View style={[styles.container, { paddingTop: insets.top + HEADER_HEIGHT, paddingLeft: insets.left, paddingRight: insets.right }]}>
      <Text testID="size" style={styles.line}>
        {landscape ? 'landscape' : 'portrait'} {Math.round(width)}x{Math.round(height)} · {changes} changes
      </Text>
      <Text testID="insets" style={styles.line}>
        insets {insets.top},{insets.left},{insets.bottom},{insets.right}
      </Text>
      <View testID="bar" style={styles.bar} />
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#fff' },
  line: { fontSize: 17, padding: 12 },
  bar: { height: 40, backgroundColor: '#0f766e' },
});
