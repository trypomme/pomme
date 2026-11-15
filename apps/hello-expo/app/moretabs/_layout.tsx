import { NativeTabs } from 'expo-router/native-tabs';

// Seven native tabs: four on the bar and More, whose list has the rest
// (react-native-screens hooks the More navigation controller's pushes).
// Reached with the pommehello:///moretabs link.
export default function MoreTabsLayout() {
  return (
    <NativeTabs>
      <NativeTabs.Trigger name="one">
        <NativeTabs.Trigger.Icon sf="1.circle" md="counter_1" />
        <NativeTabs.Trigger.Label>One</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="two">
        <NativeTabs.Trigger.Icon sf="2.circle" md="counter_2" />
        <NativeTabs.Trigger.Label>Two</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="three">
        <NativeTabs.Trigger.Icon sf="3.circle" md="counter_3" />
        <NativeTabs.Trigger.Label>Three</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="four">
        <NativeTabs.Trigger.Icon sf="4.circle" md="counter_4" />
        <NativeTabs.Trigger.Label>Four</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="five">
        <NativeTabs.Trigger.Icon sf="5.circle" md="counter_5" />
        <NativeTabs.Trigger.Label>Five</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="six">
        <NativeTabs.Trigger.Icon sf="6.circle" md="counter_6" />
        <NativeTabs.Trigger.Label>Six</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="seven">
        <NativeTabs.Trigger.Icon sf="7.circle" md="counter_7" />
        <NativeTabs.Trigger.Label>Seven</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
    </NativeTabs>
  );
}
