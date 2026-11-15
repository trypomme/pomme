import { StyleSheet, Text, View } from 'react-native';

export default function ThreeTab() {
  return (
    <View style={styles.page}>
      <Text style={styles.title}>Three tab</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1, backgroundColor: '#fff', paddingTop: 120 },
  title: { fontSize: 28, fontWeight: '700', padding: 16 },
});
