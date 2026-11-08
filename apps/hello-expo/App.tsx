import { useState } from 'react';
import { StatusBar } from 'expo-status-bar';
import { Pressable, StyleSheet, Text, View } from 'react-native';

export default function App() {
  const [taps, setTaps] = useState(0);
  return (
    <View style={styles.container}>
      <Text>Hello from pomme 🍎</Text>
      <Pressable
        testID="tap-button"
        onPress={() => setTaps((n) => n + 1)}
        style={({ pressed }) => [styles.button, pressed && styles.pressed]}
      >
        <Text style={styles.label}>Tapped {taps} {taps === 1 ? 'time' : 'times'}</Text>
      </Pressable>
      <StatusBar style="auto" />
    </View>
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
});
