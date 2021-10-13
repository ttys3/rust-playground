import React, { Suspense, useMemo } from 'react';
import { useSelector } from 'react-redux';
import { aceKeybinding, aceResizeKey, offerCrateAutocompleteOnUse } from './selectors';

import State from './state';
import { CommonEditorProps } from './types';

const loadAceEditor = ({ keybinding, theme }) => React.lazy(() => {
  // It's important to load the core Ace library *first*, before
  // loading themes / keybindings. Without that, we get an error on an
  // uncached page load.
  return import('./AdvancedEditorCore').then((core) => {
    const kp = keybinding ? import(
      /* webpackChunkName: "ace-[request]" */
      `ace-builds/src-noconflict/keybinding-${keybinding}`
    ) : Promise.resolve();

    const tp = import(
      /* webpackChunkName: "ace-[request]" */
      `ace-builds/src-noconflict/theme-${theme}`
    );

    return Promise.all([kp, tp]).then(() => core);
  });
});

const AdvancedEditor: React.SFC<CommonEditorProps> = props => {
  const autocompleteOnUse = useSelector(offerCrateAutocompleteOnUse);
  const keybinding = useSelector(aceKeybinding);
  const pairCharacters = useSelector((state: State) => state.configuration.pairCharacters);
  const resizeKey = useSelector(aceResizeKey);
  const theme = useSelector((state: State) => state.configuration.theme);

  const AceEditor = useMemo(() => loadAceEditor({ keybinding, theme }), [keybinding, theme]);

  return (
    <Suspense fallback={<Fallback />}>
      <AceEditor
        autocompleteOnUse={autocompleteOnUse}
        keybinding={keybinding}
        pairCharacters={pairCharacters}
        resizeKey={resizeKey}
        theme={theme}
        {...props} />
    </Suspense>
  );
}

const Fallback: React.SFC = () => (
  <div>Loading the ACE editor...</div>
);

export default AdvancedEditor;
