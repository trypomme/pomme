import { useState } from 'react';
import { Stack } from 'expo-router';
import { Pressable, StyleSheet, Text, TextInput, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

// Multiline text input: a TextInput with `multiline` (a UITextView on iOS)
// that grows with its text up to a maximum height, then scrolls. Reached with
// the pommehello:///notes link. A Clear button in the header (Expo Router's
// `headerRight`: a bar button item holding React Native's view) empties it.

export default function NotesScreen() {
  const [notes, setNotes] = useState('');
  const [focused, setFocused] = useState(false);
  // The content height React Native reports as the text grows.
  const [height, setHeight] = useState(0);
  const lines = notes ? notes.split('\n').length : 0;
  // The screen runs under the (transparent) header: start below it.
  const top = useSafeAreaInsets().top + HEADER_HEIGHT;
  return (
    <View style={[styles.container, { paddingTop: top + 16 }]}>
      <Stack.Screen
        options={{
          headerRight: () => (
            <Pressable testID="clear" accessibilityRole="button" onPress={() => setNotes('')} hitSlop={8}>
              <Text style={styles.clear}>Clear</Text>
            </Pressable>
          ),
        }}
      />
      <TextInput
        testID="notes"
        style={styles.input}
        multiline
        placeholder="Write a note"
        value={notes}
        onChangeText={setNotes}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onContentSizeChange={(e) => setHeight(Math.round(e.nativeEvent.contentSize.height))}
        autoCorrect={false}
      />
      <Text testID="summary" style={styles.summary}>
        {focused ? 'Editing' : 'Not editing'} · {lines} lines · {notes.length} characters
      </Text>
      <Text testID="height" style={styles.summary}>{height ? `content ${height}` : 'content not measured'}</Text>
    </View>
  );
}

const HEADER_HEIGHT = 44;

const styles = StyleSheet.create({
  container: { flex: 1, padding: 16, gap: 12, backgroundColor: '#fff' },
  input: {
    minHeight: 44,
    maxHeight: 140,
    borderWidth: 1,
    borderColor: '#94a3b8',
    borderRadius: 10,
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontSize: 17,
    backgroundColor: '#fff',
  },
  summary: { fontSize: 15, color: '#475569' },
  clear: { fontSize: 17, color: '#0088ff', paddingHorizontal: 8 },
});
