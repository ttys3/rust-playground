import React from 'react';
import { CommonEditorProps } from '../types';
import MonacoReact, { Monaco, loader } from "@monaco-editor/react";
import { connect } from 'react-redux';
import State from '../state';

import styles from './Editor.module.css';
import { config, grammar } from './rust_monaco_def';

const modeId = 'my-rust';

const initMonaco = (monaco: Monaco) => {
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
    id: modeId,
  });

  monaco.languages.onLanguage(modeId, async () => {
    monaco.languages.setLanguageConfiguration(modeId, config);
    monaco.languages.setMonarchTokensProvider(modeId, grammar);
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

loader.config({ paths: { vs: '/assets/vs' } });

const MonacoEditor: React.SFC<MonacoEditorProps> = props => {
  return (
    <MonacoReact
      language={modeId}
      theme={props.theme}
      loading="Loading the Monaco editor..."
      className={styles.advanced}
      value={props.code}
      onChange={props.onEditCode}
      beforeMount={initMonaco}
    />
  );
}

export default connect<PropsFromState, undefined, CommonEditorProps>(mapStateToProps)(MonacoEditor);
