// This is used to store "long-term" values; those which we want to be
// preserved between completely independent sessions of the
// playground.

import State from './state';
import storage from './storage';

const CURRENT_VERSION = 2;

export function serialize(state: State) {
  return JSON.stringify({
    version: CURRENT_VERSION,
    configuration: {
      editor: state.configuration.editor,
      keybinding: state.configuration.keybinding,
      aceTheme: state.configuration.aceTheme,
      monacoTheme: state.configuration.monacoTheme,
      pairCharacters: state.configuration.pairCharacters,
      orientation: state.configuration.orientation,
      assemblyFlavor: state.configuration.assemblyFlavor,
      demangleAssembly: state.configuration.demangleAssembly,
      processAssembly: state.configuration.processAssembly,
    },
    code: state.code,
    notifications: state.notifications,
  });
}

export function deserialize(savedState) {
  if (!savedState) { return undefined; }
  const parsedState = JSON.parse(savedState);
  if (!parsedState) { return undefined; }
  let { version } = parsedState;
  delete parsedState.version;

  // migrations
  if (version === 1) {
    if (parsedState.editor === 'advanced') {
      parsedState.editor = 'ace';
    }
    parsedState.aceTheme = parsedState.theme;
    parsedState.monacoTheme = 'vscode-dark-plus';
    delete parsedState.theme;
    version = 2;
  }

  // This assumes that the keys we serialize with match the keys in the
  // live state. If that's no longer true, an additional renaming step
  // needs to be added.
  return parsedState;
}

export default storage({
  storageFactory: () => localStorage,
  serialize,
  deserialize,
});
