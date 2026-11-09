import * as Network from 'expo-network';
import { useEffect, useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

// expo-network: the network state as useNetworkState sees it (and as it
// changes), and the device's IP address. Reached with the pommehello:///network link.

const HEADER_HEIGHT = 44;

export default function NetworkScreen() {
  const top = useSafeAreaInsets().top + HEADER_HEIGHT;
  const state = useNetworkState();
  const [ip, setIp] = useState('…');
  useEffect(() => {
    Network.getIpAddressAsync().then(setIp, (e) => setIp(`error ${e}`));
  }, []);
  return (
    <View style={[styles.page, { paddingTop: top }]}>
      <Text testID="state" style={styles.line}>
        {state.type ?? '…'} · connected {String(state.isConnected)} · reachable {String(state.isInternetReachable)} · {state.changes} updates
      </Text>
      <Text testID="ip" style={styles.line}>
        ip {ip}
      </Text>
    </View>
  );
}

/** useNetworkState, also counting the listener's updates. */
function useNetworkState() {
  const [state, setState] = useState<Network.NetworkState & { changes: number }>({ changes: 0 });
  useEffect(() => {
    Network.getNetworkStateAsync().then((s) => setState((old) => ({ ...s, changes: old.changes })));
    const sub = Network.addNetworkStateListener((s) => setState((old) => ({ ...s, changes: old.changes + 1 })));
    return () => sub.remove();
  }, []);
  return state;
}

const styles = StyleSheet.create({
  page: { flex: 1, backgroundColor: '#fff' },
  line: { fontSize: 17, padding: 12 },
});
