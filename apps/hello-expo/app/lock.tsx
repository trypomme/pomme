import AsyncStorage from '@react-native-async-storage/async-storage';
import * as LocalAuthentication from 'expo-local-authentication';
import * as SecureStore from 'expo-secure-store';
import { useCallback, useEffect, useState } from 'react';
import { Pressable, StyleSheet, Switch, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

// A pretend app lock: choose a 4-digit PIN (kept in the Keychain with
// expo-secure-store), then unlock with it on the keypad, or with Face ID /
// Touch ID (expo-local-authentication) once "Unlock with Face ID" is on (a
// setting kept in AsyncStorage; turning it on asks for a face first).
// Reached with the pommehello:///lock link.

const HEADER_HEIGHT = 44;
const PIN_LENGTH = 4;
const KEYS = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '', '0', '⌫'];

type Mode = 'loading' | 'choose' | 'locked' | 'unlocked';

export default function LockScreen() {
  const top = useSafeAreaInsets().top + HEADER_HEIGHT;
  const [mode, setMode] = useState<Mode>('loading');
  const [entry, setEntry] = useState('');
  const [status, setStatus] = useState('');
  const [biometricsOn, setBiometricsOn] = useState(false);
  const [biometryName, setBiometryName] = useState('Face ID');
  const [enrolled, setEnrolled] = useState(false);

  useEffect(() => {
    (async () => {
      const types = await LocalAuthentication.supportedAuthenticationTypesAsync();
      setBiometryName(types.includes(LocalAuthentication.AuthenticationType.FACIAL_RECOGNITION) ? 'Face ID' : 'Touch ID');
      setEnrolled(await LocalAuthentication.isEnrolledAsync());
      setBiometricsOn((await AsyncStorage.getItem('useBiometrics')) === 'on');
      const pin = await SecureStore.getItemAsync('pin');
      setMode(pin ? 'locked' : 'choose');
      setStatus(pin ? 'Enter your PIN' : 'Choose a PIN');
    })();
  }, []);

  const submit = useCallback(
    async (pin: string) => {
      setEntry('');
      if (mode === 'choose') {
        await SecureStore.setItemAsync('pin', pin);
        setMode('locked');
        setStatus('PIN saved. Enter your PIN');
      } else if (mode === 'locked') {
        if (pin === (await SecureStore.getItemAsync('pin'))) {
          setMode('unlocked');
          setStatus('Unlocked with PIN');
        } else {
          setStatus('Wrong PIN');
        }
      }
    },
    [mode],
  );

  const press = (key: string) => {
    if (key === '⌫') {
      setEntry((e) => e.slice(0, -1));
      return;
    }
    if (!key) return;
    const next = entry + key;
    setEntry(next);
    if (next.length === PIN_LENGTH) submit(next);
  };

  const askForFace = async (reason: string) => {
    const result = await LocalAuthentication.authenticateAsync({ promptMessage: reason, disableDeviceFallback: true, cancelLabel: 'Use PIN' });
    return result.success ? 'ok' : result.error;
  };

  const unlockWithBiometrics = async () => {
    const r = await askForFace('Unlock pomme');
    if (r === 'ok') {
      setMode('unlocked');
      setStatus(`Unlocked with ${biometryName}`);
    } else {
      setStatus(`${biometryName} failed: ${r}`);
    }
  };

  const toggleBiometrics = async (on: boolean) => {
    if (on) {
      if (!enrolled) {
        setStatus(`${biometryName} isn't set up`);
        return;
      }
      const r = await askForFace(`Use ${biometryName} to unlock`);
      if (r !== 'ok') {
        setStatus(`${biometryName} failed: ${r}`);
        return;
      }
    }
    await AsyncStorage.setItem('useBiometrics', on ? 'on' : 'off');
    setBiometricsOn(on);
    setStatus(on ? `${biometryName} on` : `${biometryName} off`);
  };

  const lock = () => {
    setMode('locked');
    setStatus('Locked');
  };

  const showPad = mode === 'choose' || mode === 'locked';
  return (
    <View style={[styles.page, { paddingTop: top }]}>
      <Text testID="status" style={styles.status}>
        {status}
      </Text>
      {showPad && (
        <>
          <View style={styles.dots}>
            {Array.from({ length: PIN_LENGTH }, (_, i) => (
              <View key={i} style={[styles.dot, i < entry.length && styles.dotFilled]} />
            ))}
          </View>
          <View style={styles.pad}>
            {KEYS.map((k, i) => (
              <Pressable key={i} role="button" aria-hidden={!k} aria-label={k === '⌫' ? 'Delete' : undefined} onPress={() => press(k)} style={[styles.key, !k && styles.keyBlank]} disabled={!k}>
                <Text style={styles.keyLabel}>{k}</Text>
              </Pressable>
            ))}
          </View>
          {mode === 'locked' && biometricsOn && enrolled && (
            <Pressable role="button" onPress={unlockWithBiometrics} style={styles.button}>
              <Text style={styles.buttonLabel}>Unlock with {biometryName}</Text>
            </Pressable>
          )}
        </>
      )}
      {mode === 'unlocked' && (
        <View style={styles.settings}>
          <View style={styles.row}>
            <Text style={styles.rowLabel}>Unlock with {biometryName}</Text>
            <Switch value={biometricsOn} onValueChange={toggleBiometrics} />
          </View>
          <Pressable role="button" onPress={lock} style={styles.button}>
            <Text style={styles.buttonLabel}>Lock</Text>
          </Pressable>
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1, backgroundColor: '#fff', alignItems: 'center' },
  status: { fontSize: 17, padding: 12, height: 48 },
  dots: { flexDirection: 'row', gap: 16, marginVertical: 16 },
  dot: { width: 14, height: 14, borderRadius: 7, borderWidth: 1.5, borderColor: '#111' },
  dotFilled: { backgroundColor: '#111' },
  pad: { width: 264, flexDirection: 'row', flexWrap: 'wrap', gap: 12 },
  key: { width: 80, height: 80, borderRadius: 40, backgroundColor: '#e5e7eb', alignItems: 'center', justifyContent: 'center' },
  keyBlank: { backgroundColor: 'transparent' },
  keyLabel: { fontSize: 28 },
  button: { marginTop: 24, paddingHorizontal: 20, paddingVertical: 12, borderRadius: 12, backgroundColor: '#2563eb' },
  buttonLabel: { color: '#fff', fontSize: 17 },
  settings: { width: '100%', paddingHorizontal: 20, alignItems: 'center' },
  row: { width: '100%', flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', paddingVertical: 12 },
  rowLabel: { fontSize: 17 },
});
