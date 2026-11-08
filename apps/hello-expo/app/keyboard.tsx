import { useState } from 'react';
import { KeyboardAvoidingView, StyleSheet, Text, TextInput, View } from 'react-native';

// Text input and the keyboard: a TextInput pinned to the bottom of a
// KeyboardAvoidingView (so the keyboard would cover it unless the view moves
// up), echoing what's typed. Reached with the pommehello:///keyboard link.

export default function KeyboardScreen() {
  const [name, setName] = useState('');
  const [submitted, setSubmitted] = useState('');
  const [focused, setFocused] = useState(false);
  return (
    <KeyboardAvoidingView testID="avoider" behavior="padding" keyboardVerticalOffset={100} style={styles.container}>
      <View style={styles.top}>
        <Text testID="greeting" style={styles.greeting}>{name ? `Hello, ${name}!` : 'Type your name below'}</Text>
        <Text testID="state" style={styles.state}>
          {focused ? 'Editing' : 'Not editing'} · {name.length} characters{submitted ? ` · submitted "${submitted}"` : ''}
        </Text>
      </View>
      <View style={styles.bottom}>
        <TextInput
          testID="name"
          style={styles.input}
          placeholder="Your name"
          value={name}
          onChangeText={setName}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          onSubmitEditing={(e) => setSubmitted(e.nativeEvent.text)}
          returnKeyType="done"
          autoCorrect={false}
        />
      </View>
    </KeyboardAvoidingView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#fff' },
  top: { flex: 1, alignItems: 'center', justifyContent: 'center', gap: 8, padding: 24 },
  greeting: { fontSize: 22, fontWeight: '600' },
  state: { fontSize: 15, color: '#475569' },
  bottom: { padding: 16, borderTopWidth: 1, borderTopColor: '#e2e8f0', backgroundColor: '#f8fafc' },
  input: { height: 44, borderWidth: 1, borderColor: '#94a3b8', borderRadius: 10, paddingHorizontal: 12, fontSize: 17, backgroundColor: '#fff' },
});
