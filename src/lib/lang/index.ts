import en from './en.json';
import hu from './hu.json';

// [texts, right-to-left]
export const langs: Record<string, [Record<string, string>, boolean]> = {
	en: [en, false],
	hu: [hu, false]
};
