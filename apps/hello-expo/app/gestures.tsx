import { useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { GestureDetector, GestureHandlerRootView, usePanGesture, useTapGesture } from 'react-native-gesture-handler';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

// react-native-gesture-handler: a box dragged with a pan gesture (UIKit's pan
// recognizer underneath) and a tap target. Reached with the
// pommehello:///gestures link.

const HEADER_HEIGHT = 44;

export default function GesturesScreen() {
  const top = useSafeAreaInsets().top + HEADER_HEIGHT;
  // Where the box rests, and how far the current drag has moved it.
  const [rest, setRest] = useState({ x: 0, y: 0 });
  const [drag, setDrag] = useState({ x: 0, y: 0 });
  const [status, setStatus] = useState('idle');
  const [taps, setTaps] = useState(0);
  const pan = usePanGesture({
    onActivate: () => setStatus('dragging'),
    onUpdate: (e) => setDrag({ x: e.translationX, y: e.translationY }),
    onDeactivate: (e) => {
      setRest((r) => ({ x: r.x + e.translationX, y: r.y + e.translationY }));
      setDrag({ x: 0, y: 0 });
      setStatus(`dropped at ${Math.round(e.translationX)},${Math.round(e.translationY)}`);
    },
  });
  const tap = useTapGesture({ onActivate: () => setTaps((n) => n + 1) });
  const x = rest.x + drag.x;
  const y = rest.y + drag.y;
  return (
    <GestureHandlerRootView style={[styles.container, { paddingTop: top + 16 }]}>
      <Text testID="status" style={styles.status}>
        {status} · box {Math.round(x)},{Math.round(y)} · {taps} taps
      </Text>
      <GestureDetector gesture={tap}>
        <View testID="tap" style={styles.tap}>
          <Text style={styles.label}>Tap me</Text>
        </View>
      </GestureDetector>
      <GestureDetector gesture={pan}>
        <View testID="box" style={[styles.box, { transform: [{ translateX: x }, { translateY: y }] }]} />
      </GestureDetector>
    </GestureHandlerRootView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#fff', paddingHorizontal: 16, gap: 16 },
  status: { fontSize: 15, color: '#475569' },
  tap: { height: 60, borderRadius: 12, backgroundColor: '#e0e7ff', alignItems: 'center', justifyContent: 'center' },
  label: { fontSize: 17, color: '#3730a3' },
  box: { width: 120, height: 120, borderRadius: 16, backgroundColor: '#f97316' },
});
