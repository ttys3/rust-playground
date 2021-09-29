// This is used to store "long-term" values; those which we want to be
// preserved between completely independent sessions of the
// playground.

import State from './state';
import storage from './storage';
import { AssemblyFlavor, DemangleAssembly, Editor, Orientation, PairCharacters, ProcessAssembly } from './types';

const CURRENT_VERSION = 2;

type V2Configuration = {
  version: 2;
  configuration: {
    editor: Editor;
    keybinding: string;
    aceTheme: string;
    monacoTheme: string;
    pairCharacters: PairCharacters;
    orientation: Orientation;
    assemblyFlavor: AssemblyFlavor;
    demangleAssembly: DemangleAssembly;
    processAssembly: ProcessAssembly;
  };
  code: string;
  notifications: any;
};

type V1Configuration = {
  version: 1;
  configuration: {
    editor: "simple" | "advanced";
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
};

type CurrentConfiguration = V2Configuration;

export function serialize(state: State): string {
  const conf: CurrentConfiguration = {
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
  };
  return JSON.stringify(conf);
}

/*
Sample configuration for each version, for debug propose

"{\"version\":1,\"configuration\":{\"editor\":\"advanced\",\"keybinding\":\"ace\",\"theme\":\"github\",\"pairCharacters\":\"enabled\",\"orientation\":\"automatic\",\"assemblyFlavor\":\"att\",\"demangleAssembly\":\"demangle\",\"processAssembly\":\"filter\"},\"code\":\"fn main() {\\n    println!(\\\"Hello, world!\\\");\\n}\",\"notifications\":{\"seenRustSurvey2018\":true,\"seenRust2018IsDefault\":true,\"seenRustSurvey2020\":false}}"
"{\"version\":2,\"configuration\":{\"editor\":\"ace\",\"keybinding\":\"ace\",\"aceTheme\":\"github\",\"monacoTheme\":\"vscode-dark-plus\",\"pairCharacters\":\"enabled\",\"orientation\":\"automatic\",\"assemblyFlavor\":\"att\",\"demangleAssembly\":\"demangle\",\"processAssembly\":\"filter\"},\"code\":\"fn main() {\\n    println!(\\\"Hello, world!\\\");\\n}\",\"notifications\":{\"seenRustSurvey2018\":true,\"seenRust2018IsDefault\":true,\"seenRustSurvey2020\":false}}"
*/

function migrate1(state: V1Configuration): CurrentConfiguration {
  const { theme, editor, ...configuration } = state.configuration;
  const step: V2Configuration = {
    ...state,
    configuration: {
      ...configuration,
      aceTheme: theme,
      monacoTheme: 'vscode-dark-plus',
      editor: editor === 'advanced' ? Editor.Ace : Editor.Simple,
    },
    version: 2,
  };
  return migrate2(step);
}

function migrate2(state: V2Configuration): CurrentConfiguration {
  return state;
}

export function deserialize(savedState: string) {
  if (!savedState) { return undefined; }
  const parsedState: V1Configuration | V2Configuration = JSON.parse(savedState);
  if (!parsedState) { return undefined; }
  let result: CurrentConfiguration;

  if (parsedState.version === 1) result = migrate1(parsedState);
  if (parsedState.version === 2) result = migrate2(parsedState);

  delete result.version;

  // There is some keys that we don't store (why?) so type checker needs to be
  // suppressed here
  return result as any;
}

export default storage({
  storageFactory: () => localStorage,
  serialize,
  deserialize,
});
