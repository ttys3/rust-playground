import React from 'react';
import { CommonEditorProps } from '../types';
//import MonacoReact, { Monaco, loader } from "@monaco-editor/react";
import MonacoEditor from 'react-monaco-editor';
import { connect } from 'react-redux';
import State from '../state';

import styles from './Editor.module.css';
import { config, grammar } from './rust_monaco_def';

const MODE_ID = 'my-rust';

// monaco.editor.IStandaloneEditorConstructionOptions

const initMonaco = (monaco: any) => { // TODO: type
  console.log("initMonaco");
  monaco.editor.defineTheme('vscode-dark-plus', {
    base: 'vs-dark',
    inherit: true,
    colors: {},
    rules: [
      { token: 'keyword.control', foreground: 'C586C0' },
      { token: 'variable', foreground: '9CDCFE' },
      { token: 'support.function', foreground: 'DCDCAA' },
    ]
  });
  monaco.languages.register({ // language for editor
    id: MODE_ID,
  });

  monaco.languages.onLanguage(MODE_ID, async () => {
    monaco.languages.setLanguageConfiguration(MODE_ID, config);
    monaco.languages.setMonarchTokensProvider(MODE_ID, grammar);
  });
};

interface PropsFromState {
  theme: string;
}

const mapStateToProps = (state: State) => {
  const { configuration: { monacoTheme: theme, } } = state;
  return { theme };
};


type MonacoEditorProps = CommonEditorProps & PropsFromState;

const MonacoEditorCore: React.SFC<MonacoEditorProps> = props => {
  return (
    <MonacoEditor
      language={MODE_ID}
      theme={props.theme}
      className={styles.advanced}
      value={props.code}
      onChange={props.onEditCode}
      editorWillMount={initMonaco}
    />
  );
}

export default connect<PropsFromState, undefined, CommonEditorProps>(mapStateToProps)(MonacoEditorCore);
