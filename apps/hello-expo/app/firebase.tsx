import { getAnalytics, getAppInstanceId, logEvent } from '@react-native-firebase/analytics';
import { getApp } from '@react-native-firebase/app';
import { getAuth, signInAnonymously } from '@react-native-firebase/auth';
import { getCrashlytics, log, setAttribute } from '@react-native-firebase/crashlytics';
import { doc, getDoc, getDocFromCache, getFirestore, setDoc } from '@react-native-firebase/firestore';
import { useEffect, useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

// Firebase through React Native Firebase: the native Firebase Apple SDK
// configured from GoogleService-Info.plist at launch. Each line runs one
// module and shows what came back. Reached with the pommehello:///firebase link.

const HEADER_HEIGHT = 44;

async function app(): Promise<string> {
  const a = getApp();
  return `${a.name} · ${a.options.projectId} · ${a.options.bundleId}`;
}

async function analytics(): Promise<string> {
  const a = getAnalytics();
  await logEvent(a, 'pomme_opened', { screen: 'firebase' });
  return `logged · instance ${(await getAppInstanceId(a)) ?? 'none'}`;
}

async function crashlytics(): Promise<string> {
  const c = getCrashlytics();
  log(c, 'opened the Firebase screen');
  await setAttribute(c, 'screen', 'firebase');
  return `logged · collection ${c.isCrashlyticsCollectionEnabled}`;
}

// One anonymous sign-in for the screen. Firestore waits for it: its rules
// allow signed-in users only, and writes made before a sign-in belong to the
// signed-out user (they'd stay queued once the user changes).
let signingIn: ReturnType<typeof signInAnonymously> | undefined;
function signIn() {
  signingIn ??= signInAnonymously(getAuth());
  return signingIn;
}

async function auth(): Promise<string> {
  const { user } = await signIn();
  return `anonymous ${user.isAnonymous} · ${user.uid.length} char uid`;
}

/** A write and a read of one document: the local cache first, then the server. */
async function firestore(): Promise<string> {
  await signIn();
  const ref = doc(getFirestore(), 'pomme', 'hello');
  const at = Date.now();
  const written = setDoc(ref, { greeting: 'hello', at });
  const cached = (await getDocFromCache(ref)).data();
  await written;
  const server = (await getDoc(ref)).data();
  return `cache ${cached?.greeting} · server ${server?.at === at ? 'same' : 'different'}`;
}

function useLine(label: string, run: () => Promise<string>): string {
  const [line, setLine] = useState(`${label} …`);
  useEffect(() => {
    run().then((s) => setLine(`${label} ${s}`), (e) => setLine(`${label} error ${e?.code ?? ''} ${e?.message ?? e}`));
  }, []);
  return line;
}

export default function FirebaseScreen() {
  const top = useSafeAreaInsets().top + HEADER_HEIGHT;
  const lines = [
    ['app', useLine('app', app)],
    ['analytics', useLine('analytics', analytics)],
    ['crashlytics', useLine('crashlytics', crashlytics)],
    ['auth', useLine('auth', auth)],
    ['firestore', useLine('firestore', firestore)],
  ];
  return (
    <View style={[styles.page, { paddingTop: top }]}>
      {lines.map(([id, line]) => (
        <Text key={id} testID={id} style={styles.line}>{line}</Text>
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1, backgroundColor: '#fff' },
  line: { fontSize: 15, padding: 12 },
});
