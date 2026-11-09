import { useState } from 'react';
import { StyleSheet, Text, TextInput, View } from 'react-native';
import { KeyboardProvider, KeyboardStickyView, useKeyboardHandler } from 'react-native-keyboard-controller';
import { scheduleOnRN } from 'react-native-worklets';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

// react-native-keyboard-controller: a message field stuck to the top of the
// keyboard, following it frame by frame as it slides (a worklet on the UI
// thread, through Reanimated). The status line shows what the keyboard
// handler saw. Reached with the pommehello:///sticky link.

const HEADER_HEIGHT = 44;

// The provider wraps just this screen, so the rest of the app runs without it.
export default function StickyScreen() {
  return (
    <KeyboardProvider>
      <Sticky />
    </KeyboardProvider>
  );
}

function Sticky() {
  const top = useSafeAreaInsets().top + HEADER_HEIGHT;
  const [status, setStatus] = useState('keyboard down');
  const [moves, setMoves] = useState(0);
  const report = (height: number, progress: number, frames: number) => {
    setStatus(`keyboard ${Math.round(height)} at ${Math.round(progress * 100)}%`);
    setMoves(frames);
  };
  useKeyboardHandler(
    {
      onMove: (e) => {
        'worklet';
        globalThis.__moves = (globalThis.__moves ?? 0) + 1;
      },
      onEnd: (e) => {
        'worklet';
        scheduleOnRN(report, e.height, e.progress, globalThis.__moves ?? 0);
      },
    },
    [],
  );
  return (
    <View style={[styles.container, { paddingTop: top + 16 }]}>
      <Text testID="status" style={styles.status}>
        {status} · {moves} moves
      </Text>
      <KeyboardStickyView style={styles.sticky}>
        <TextInput testID="message" style={styles.input} placeholder="Message" autoCorrect={false} />
      </KeyboardStickyView>
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#fff', paddingHorizontal: 16 },
  status: { fontSize: 15, color: '#475569' },
  sticky: { position: 'absolute', left: 0, right: 0, bottom: 0, padding: 16, backgroundColor: '#f8fafc' },
  input: { height: 44, borderWidth: 1, borderColor: '#94a3b8', borderRadius: 10, paddingHorizontal: 12, fontSize: 17, backgroundColor: '#fff' },
});
