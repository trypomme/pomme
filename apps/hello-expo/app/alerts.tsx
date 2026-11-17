import { useState } from 'react';
import { ActionSheetIOS, Alert, Pressable, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

// React Native's alerts: Alert.alert (an alert with two buttons),
// Alert.prompt (one with a text field) and ActionSheetIOS (an action sheet
// with a destructive choice and a cancel),
// both UIAlertController underneath. The screen says which button was
// chosen. Reached with the pommehello:///alerts link.

const HEADER_HEIGHT = 44;

export default function AlertsScreen() {
  const top = useSafeAreaInsets().top + HEADER_HEIGHT;
  const [chosen, setChosen] = useState('Nothing chosen yet');

  const showAlert = () =>
    Alert.alert('Delete the note?', "This can't be undone.", [
      { text: 'Cancel', style: 'cancel', onPress: () => setChosen('Alert: Cancel') },
      { text: 'Delete', style: 'destructive', onPress: () => setChosen('Alert: Delete') },
    ]);

  const showPrompt = () =>
    Alert.prompt('Your name', undefined, (name) => setChosen(`Prompt: ${name}`));

  const showSheet = () =>
    ActionSheetIOS.showActionSheetWithOptions(
      { title: 'Share the note', options: ['Cancel', 'Copy Link', 'Delete'], cancelButtonIndex: 0, destructiveButtonIndex: 2 },
      (i) => setChosen(`Sheet: ${['Cancel', 'Copy Link', 'Delete'][i]}`),
    );

  return (
    <View style={[styles.screen, { paddingTop: top + 24 }]}>
      <Pressable testID="show-alert" style={styles.button} onPress={showAlert}>
        <Text style={styles.buttonText}>Show alert</Text>
      </Pressable>
      <Pressable testID="show-prompt" style={styles.button} onPress={showPrompt}>
        <Text style={styles.buttonText}>Show prompt</Text>
      </Pressable>
      <Pressable testID="show-sheet" style={styles.button} onPress={showSheet}>
        <Text style={styles.buttonText}>Show action sheet</Text>
      </Pressable>
      <Text testID="chosen" style={styles.chosen}>
        {chosen}
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, alignItems: 'center', gap: 16, backgroundColor: '#fff' },
  button: { backgroundColor: '#0a84ff', borderRadius: 10, paddingHorizontal: 20, paddingVertical: 12 },
  buttonText: { color: '#fff', fontSize: 17, fontWeight: '600' },
  chosen: { fontSize: 17, marginTop: 16 },
});
