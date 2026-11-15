import { ScrollView, StyleSheet, Text } from 'react-native';

export default function HomeTab() {
  return (
    <ScrollView contentInsetAdjustmentBehavior="automatic" style={styles.page}>
      <Text testID="tab" style={styles.title}>Home tab</Text>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1, backgroundColor: '#fff' },
  title: { fontSize: 28, fontWeight: '700', padding: 16 },
});
