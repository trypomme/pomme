import { useState } from 'react';
import { FlatList, Pressable, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

// Scrolling: a FlatList of 200 rows (rendered lazily as they scroll into
// view), reporting where it came to rest and which row was last tapped.
// Reached with the pommehello:///list link. (The offset is shown when a drag
// or glide ends, not on every scroll event: re-rendering the screen 60 times
// a second is expensive under emulation.)

const ROWS = Array.from({ length: 200 }, (_, i) => `Row ${i}`);
const ROW_HEIGHT = 50;
const HEADER_HEIGHT = 44;

export default function ListScreen() {
  const [offset, setOffset] = useState(0);
  const [dragging, setDragging] = useState(false);
  const [tapped, setTapped] = useState('');
  const top = useSafeAreaInsets().top + HEADER_HEIGHT;
  return (
    <View style={[styles.container, { paddingTop: top }]}>
      <Text testID="status" style={styles.status}>
        offset {Math.round(offset)} · {dragging ? 'dragging' : 'still'} · {tapped ? `tapped ${tapped}` : 'no taps'}
      </Text>
      <FlatList
        testID="list"
        data={ROWS}
        keyExtractor={(item) => item}
        getItemLayout={(_, index) => ({ length: ROW_HEIGHT, offset: ROW_HEIGHT * index, index })}
        onScrollBeginDrag={() => setDragging(true)}
        onScrollEndDrag={(e) => {
          setDragging(false);
          setOffset(e.nativeEvent.contentOffset.y);
        }}
        onMomentumScrollEnd={(e) => setOffset(e.nativeEvent.contentOffset.y)}
        renderItem={({ item, index }) => (
          <Pressable role="button" onPress={() => setTapped(item)} style={[styles.row, index % 2 ? styles.odd : null]}>
            <Text style={styles.label}>{item}</Text>
          </Pressable>
        )}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#fff' },
  status: { fontSize: 15, color: '#475569', padding: 12 },
  row: { height: ROW_HEIGHT, justifyContent: 'center', paddingHorizontal: 16 },
  odd: { backgroundColor: '#f1f5f9' },
  label: { fontSize: 17 },
});
