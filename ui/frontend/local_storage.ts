// This is used to store "long-term" values; those which we want to be
// preserved between completely independent sessions of the
// playground.

import State from './state';
import storage from './storage';
import { AssemblyFlavor, DemangleAssembly, Editor, Orientation, PairCharacters, ProcessAssembly } from './types';

const CURRENT_VERSION = 2;

interface V2Configuration {
  version: 2;
  configuration: {
    editor: Editor;
    ace: {
      keybinding: string;
      theme: string;
      pairCharacters: PairCharacters;
    }
    orientation: Orientation;
    assemblyFlavor: AssemblyFlavor;
    demangleAssembly: DemangleAssembly;
    processAssembly: ProcessAssembly;
  };
  code: string;
  notifications: any;
}

interface V1Configuration {
  version: 1;
  configuration: {
    editor: 'simple' | 'advanced';
    keybinding: string;
    theme: string;
    pairCharacters: PairCharacters;
    orientation: Orientation;
    assemblyFlavor: AssemblyFlavor;
    demangleAssembly: DemangleAssembly;
    processAssembly: ProcessAssembly;
  };
  code: string;
  notifications: any;
}

type CurrentConfiguration = V2Configuration;

export function serialize(state: State): string {
  const conf: CurrentConfiguration = {
    version: CURRENT_VERSION,
    configuration: {
      editor: state.configuration.editor,
      ace: {
        keybinding: state.configuration.ace.keybinding,
        theme: state.configuration.ace.theme,
        pairCharacters: state.configuration.ace.pairCharacters,
      },
      orientation: state.configuration.orientation,
      assemblyFlavor: state.configuration.assemblyFlavor,
      demangleAssembly: state.configuration.demangleAssembly,
      processAssembly: state.configuration.processAssembly,
    },
    code: state.code,
    notifications: state.notifications,
  };
  return JSON.stringify(conf);
}

function migrateV1(state: V1Configuration): CurrentConfiguration {
  const { editor, theme, keybinding, pairCharacters, ...configuration } = state.configuration;
  const step: V2Configuration = {
    ...state,
    configuration: {
      ...configuration,
      ace: { theme, keybinding, pairCharacters },
      editor: editor === 'advanced' ? Editor.Ace : Editor.Simple,
    },
    version: 2,
  };
  return migrateV2(step);
}

function migrateV2(state: V2Configuration): CurrentConfiguration {
  return state;
}

function migrate(state: V1Configuration | V2Configuration): CurrentConfiguration {
  switch (state.version) {
    case 1: return migrateV1(state);
    case 2: return migrateV2(state);
    default: return undefined
  }
}

export function deserialize(savedState: string) {
  if (!savedState) { return undefined; }

  const parsedState = JSON.parse(savedState);
  if (!parsedState) { return undefined; }

  const result = migrate(parsedState);
  if (!result) { return undefined; }

  delete result.version;
  return result;
}

export default storage({
  storageFactory: () => localStorage,
  serialize,
  deserialize,
});
