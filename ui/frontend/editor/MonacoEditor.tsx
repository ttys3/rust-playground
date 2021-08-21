import React from 'react';
import { CommonEditorProps, Position } from '../types';
import MonacoReact, { Monaco } from "@monaco-editor/react";

import styles from './Editor.module.css';
import { config, grammar } from './rust_monaco_def';

class CodeByteOffsets {
  readonly code: string;
  readonly lines: string[];

  constructor(code: string) {
    this.code = code;
    this.lines = code.split('\n');
  }

  public lineToOffsets(line: number) {
    const precedingBytes = this.bytesBeforeLine(line);

    const highlightedLine = this.lines[line];
    const highlightedBytes = highlightedLine.length;

    return [precedingBytes, precedingBytes + highlightedBytes];
  }

  public rangeToOffsets(start: Position, end: Position) {
    const startBytes = this.positionToBytes(start);
    const endBytes = this.positionToBytes(end);
    return [startBytes, endBytes];
  }

  private positionToBytes(position: Position) {
    // Subtract one as this logic is zero-based and the columns are one-based
    return this.bytesBeforeLine(position.line) + position.column - 1;
  }

  private bytesBeforeLine(line: number) {
    // Subtract one as this logic is zero-based and the lines are one-based
    line -= 1;

    const precedingLines = this.lines.slice(0, line);

    // Add one to account for the newline we split on and removed
    return precedingLines.map(l => l.length + 1).reduce((a, b) => a + b);
  }
}

const modeId = 'my-rust';

const initMonaco = (monaco: Monaco) => {
  monaco.editor.defineTheme('vscode-dark-plus', {
    base: 'vs-dark',
    inherit: true,
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
    console.log(modeId);

    monaco.languages.setLanguageConfiguration(modeId, config);
    monaco.languages.setMonarchTokensProvider(modeId, grammar);
  });
};

const MonacoEditor: React.SFC<CommonEditorProps> = props => {
  return (
    <MonacoReact
      language={modeId}
      theme="vscode-dark-plus"
      loading="Loading the Monaco editor..."
      className={styles.advanced}
      value={props.code}
      onChange={props.onEditCode}
      beforeMount={initMonaco}
    />
  );
}

export default MonacoEditor;
