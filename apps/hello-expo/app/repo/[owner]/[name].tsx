import { useQuery } from '@tanstack/react-query';
import { useLocalSearchParams } from 'expo-router';
import { Text, View } from 'react-native';
import { API, fetchRepo, group, styles } from '../../../components/Home';

// A dynamic route: /repo/<owner>/<name>. TanStack Query shares its cache with
// the home screen, so the repo shows at once when it was already loaded there.
export default function RepoScreen() {
  const { owner, name } = useLocalSearchParams<{ owner: string; name: string }>();
  const { data, error } = useQuery({
    queryKey: ['repo', `${owner}/${name}`],
    queryFn: () => fetchRepo(`${API}${owner}/${name}`),
  });
  return (
    <View style={styles.container}>
      <Text testID="repo-name" style={styles.cardTitle}>{`${owner}/${name}`}</Text>
      <Text testID="repo-stars" style={styles.cardText}>
        {error ? `Couldn't load: ${error.message}` : data ? `★ ${group(data.stargazers_count)} stars` : 'Loading…'}
      </Text>
    </View>
  );
}
