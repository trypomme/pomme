import {
  Button,
  Divider,
  Form,
  Group,
  Host,
  HStack,
  Picker,
  ProgressView,
  Section,
  SecureField,
  Slider,
  Spacer,
  Text,
  TextField,
  Toggle,
  useNativeState,
} from '@expo/ui/swift-ui';
import { background, buttonStyle, clipShape, font, foregroundStyle, padding, tag } from '@expo/ui/swift-ui/modifiers';
import { useState } from 'react';

// Expo UI: SwiftUI views from React. A settings-style form whose state lives
// in React: SwiftUI shows it, and its controls report changes back.
// - a toggle, a slider (with its value beside the label) and a button that
//   counts taps, each updating the text that shows it;
// - everyday controls: a text field (echoed below it), a secure field, a
//   menu picker, progress (a bar and a spinner), a group with a divider;
// - modifiers: font, a secondary foreground style, a padded, rounded badge,
//   a plain button.

export default function App() {
  const [wifi, setWifi] = useState(true);
  const [brightness, setBrightness] = useState(0.5);
  const [taps, setTaps] = useState(0);
  const name = useNativeState('');
  const password = useNativeState('');
  const [typed, setTyped] = useState('');
  const [flavor, setFlavor] = useState('vanilla');

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
        <Section title="Everyday">
          <TextField text={name} placeholder="Name" onTextChange={setTyped} />
          <Text>{`Typed: ${typed}`}</Text>
          <SecureField text={password} placeholder="Password" />
          <Picker label="Flavor" selection={flavor} onSelectionChange={setFlavor}>
            <Text modifiers={[tag('vanilla')]}>Vanilla</Text>
            <Text modifiers={[tag('chocolate')]}>Chocolate</Text>
          </Picker>
          <ProgressView value={0.4} />
          <ProgressView />
          <Group>
            <Text>Grouped one</Text>
            <Divider />
            <Text>Grouped two</Text>
          </Group>
        </Section>
        <Section title="Modifiers">
          <Text modifiers={[font({ size: 22, weight: 'bold' })]}>Bold 22</Text>
          <Text modifiers={[foregroundStyle({ type: 'hierarchical', style: 'secondary' })]}>Secondary</Text>
          <Text modifiers={[padding({ all: 8 }), background('#ffa500'), clipShape('roundedRectangle')]}>Badge</Text>
          <Button label="Plain button" modifiers={[buttonStyle('plain')]} />
        </Section>
      </Form>
    </Host>
  );
}
