import { StatusBar } from 'expo-status-bar';
import { StyleSheet, Text, View } from 'react-native';
import { Canvas, Circle, LinearGradient, Path, RoundedRect, Skia, vec } from '@shopify/react-native-skia';

// A small, fixed Skia scene with distinct colors in known places, so pomme's
// output can be checked pixel by pixel against the iOS Simulator:
// - a dark slate rounded rectangle filling the canvas (inset 20),
// - a circle with a rose → amber linear gradient (top left → bottom right),
// - a white stroked chevron under it.

export const SIZE = 300;

const chevron = Skia.Path.MakeFromSVGString('M 90 245 L 150 205 L 210 245')!;

export default function App() {
  return (
    <View style={styles.container}>
      <Text testID="title" style={styles.title}>Hello from Skia</Text>
      <Canvas testID="canvas" style={{ width: SIZE, height: SIZE }}>
        <RoundedRect x={20} y={20} width={260} height={260} r={32} color="#1e293b" />
        <Circle cx={150} cy={125} r={70}>
          <LinearGradient start={vec(80, 55)} end={vec(220, 195)} colors={['#f43f5e', '#f59e0b']} />
        </Circle>
        <Path path={chevron} color="white" style="stroke" strokeWidth={8} strokeCap="round" strokeJoin="round" />
      </Canvas>
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
  title: {
    fontSize: 17,
  },
});
