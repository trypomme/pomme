import { useState } from 'react';
import { I18nManager, Pressable, ScrollView, StyleSheet, Text } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { localizationFacts } from '../components/localizationFacts';

// The device's language and region as JavaScript sees them
// (components/localizationFacts.ts), one `name value` line each.
// Reached with the pommehello:///localization link. Its button forces a
// right-to-left layout (`I18nManager.forceRTL`), which takes effect at the
// next launch.

const HEADER_HEIGHT = 44;

export default function LocalizationScreen() {
  const top = useSafeAreaInsets().top + HEADER_HEIGHT;
  const [forced, setForced] = useState(false);
  return (
    <ScrollView style={styles.page} contentContainerStyle={{ paddingTop: top }}>
      <Pressable
        testID="forceRTL"
        accessibilityRole="button"
        style={styles.button}
        onPress={() => {
          I18nManager.allowRTL(true);
          I18nManager.forceRTL(true);
          setForced(true);
        }}>
        <Text style={styles.buttonText}>{forced ? 'Right to left at the next launch' : 'Force right to left'}</Text>
      </Pressable>
      {localizationFacts().map(([name, value]) => (
        <Text key={name} testID={name} style={styles.line}>
          {name} {value}
        </Text>
      ))}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1, backgroundColor: '#fff' },
  line: { fontSize: 13, paddingHorizontal: 12, paddingVertical: 2 },
  button: { marginHorizontal: 12, marginVertical: 8, padding: 10, borderRadius: 8, backgroundColor: '#e2e8f0' },
  buttonText: { fontSize: 15, color: '#0f172a' },
});
