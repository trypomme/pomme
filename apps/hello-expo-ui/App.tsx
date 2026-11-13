import { Button, Form, Host, HStack, Section, Slider, Spacer, Text, Toggle } from '@expo/ui/swift-ui';
import { useState } from 'react';

// Expo UI: SwiftUI views from React. A settings-style form whose state lives
// in React: SwiftUI shows it, and its controls report changes back.
// - a toggle, a slider (with its value beside the label) and a button that
//   counts taps, each updating the text that shows it.

export default function App() {
  const [wifi, setWifi] = useState(true);
  const [brightness, setBrightness] = useState(0.5);
  const [taps, setTaps] = useState(0);

  return (
    <Host style={{ flex: 1 }}>
      <Form>
        <Section title="Expo UI">
          <Text>Hello from SwiftUI</Text>
          <Toggle label="Wi-Fi" isOn={wifi} onIsOnChange={setWifi} />
          <HStack>
            <Text>Brightness</Text>
            <Spacer />
            <Text>{`${Math.round(brightness * 100)}%`}</Text>
          </HStack>
          <Slider value={brightness} onValueChange={setBrightness} />
        </Section>
        <Section title="Taps">
          <Button label={`Tapped ${taps} ${taps === 1 ? 'time' : 'times'}`} onPress={() => setTaps((n) => n + 1)} />
        </Section>
      </Form>
    </Host>
  );
}
