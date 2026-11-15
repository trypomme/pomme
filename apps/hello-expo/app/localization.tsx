import { ScrollView, StyleSheet, Text } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { localizationFacts } from '../components/localizationFacts';

// The device's language and region as JavaScript sees them
// (components/localizationFacts.ts), one `name value` line each.
// Reached with the pommehello:///localization link.

const HEADER_HEIGHT = 44;

export default function LocalizationScreen() {
  const top = useSafeAreaInsets().top + HEADER_HEIGHT;
  return (
    <ScrollView style={styles.page} contentContainerStyle={{ paddingTop: top }}>
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
});
