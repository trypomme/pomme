import AsyncStorage from '@react-native-async-storage/async-storage';
import * as SecureStore from 'expo-secure-store';
import { useEffect, useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

// Storage: AsyncStorage (files in the data container) and expo-secure-store
// (the Keychain). Each line runs a round trip and shows what came back.
// Reached with the pommehello:///storage link.

const HEADER_HEIGHT = 44;

async function asyncStorage(): Promise<string> {
  await AsyncStorage.setItem('greeting', 'hello');
  await AsyncStorage.multiSet([
    ['a', '1'],
    ['b', '2'],
  ]);
  await AsyncStorage.mergeItem('profile', JSON.stringify({ name: 'Ada' }));
  await AsyncStorage.mergeItem('profile', JSON.stringify({ city: 'London' }));
  await AsyncStorage.removeItem('a');
  const greeting = await AsyncStorage.getItem('greeting');
  const keys = [...(await AsyncStorage.getAllKeys())].sort().join(',');
  const pairs = (await AsyncStorage.multiGet(['a', 'b'])).map(([k, v]) => `${k}=${v}`).join(' ');
  const profile = await AsyncStorage.getItem('profile');
  return `${greeting} · ${keys} · ${pairs} · ${profile}`;
}

async function secureStore(): Promise<string> {
  await SecureStore.setItemAsync('token', 'abc123');
  await SecureStore.setItemAsync('token', 'def456'); // replaces it
  await SecureStore.setItemAsync('other', 'x', { keychainService: 'elsewhere' });
  const token = await SecureStore.getItemAsync('token');
  const other = await SecureStore.getItemAsync('other');
  const otherHere = await SecureStore.getItemAsync('other', { keychainService: 'elsewhere' });
  await SecureStore.deleteItemAsync('token');
  const gone = await SecureStore.getItemAsync('token');
  const sync = SecureStore.getItem('missing');
  const available = await SecureStore.isAvailableAsync();
  return `${token} · ${other} · ${otherHere} · ${gone} · ${sync} · available ${available} · biometrics ${SecureStore.canUseBiometricAuthentication()}`;
}

/** A value only Face ID unlocks: saved, then read back (which asks for a face). */
async function protectedValue(): Promise<string> {
  await SecureStore.setItemAsync('secret', 'opensesame', { requireAuthentication: true });
  return (await SecureStore.getItemAsync('secret', { requireAuthentication: true })) ?? 'null';
}

export default function StorageScreen() {
  const top = useSafeAreaInsets().top + HEADER_HEIGHT;
  const [asyncLine, setAsyncLine] = useState('async …');
  const [secureLine, setSecureLine] = useState('secure …');
  const [authLine, setAuthLine] = useState('protected …');
  useEffect(() => {
    asyncStorage().then((s) => setAsyncLine(`async ${s}`), (e) => setAsyncLine(`async error ${e}`));
    secureStore().then((s) => setSecureLine(`secure ${s}`), (e) => setSecureLine(`secure error ${e}`));
    protectedValue().then((s) => setAuthLine(`protected ${s}`), (e) => setAuthLine(`protected error ${e}`));
  }, []);
  return (
    <View style={[styles.page, { paddingTop: top }]}>
      <Text testID="async" style={styles.line}>{asyncLine}</Text>
      <Text testID="secure" style={styles.line}>{secureLine}</Text>
      <Text testID="protected" style={styles.line}>{authLine}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1, backgroundColor: '#fff' },
  line: { fontSize: 15, padding: 12 },
});
