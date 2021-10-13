import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import ace from 'ace-builds';
import 'ace-builds/src-noconflict/ext-language_tools';
import 'ace-builds/src-noconflict/ext-searchbox';
import 'ace-builds/src-noconflict/mode-rust';

import { AceResizeKey, CommonEditorProps, Crate, PairCharacters, Position } from './types';

import styles from './Editor.module.css';

type AceEditor = import('ace-builds').Ace.Editor;
type AceCompleter = import('ace-builds').Ace.Completer;

const displayExternCrateAutocomplete = (editor: AceEditor, autocompleteOnUse: boolean) => {
  const { session } = editor;
  const pos = editor.getCursorPosition();
  const line = session.getLine(pos.row);
  const precedingText = line.slice(0, pos.column);

  return !!precedingText.match(/^\s*extern\s+crate\s*\w*$/) ||
    (autocompleteOnUse && !!precedingText.match(/^\s*use\s+(?!crate|self|super)\w*$/));
};

const buildCrateAutocompleter = (autocompleteOnUse: boolean, crates: Crate[]): AceCompleter => ({
  getCompletions: (editor, _session, _pos, _prefix, callback) => {
    let suggestions = [];

    if (displayExternCrateAutocomplete(editor, autocompleteOnUse)) {
      const len = crates.length;

      suggestions = crates.map(({ name, version, id }, i) => ({
        caption: `${name} (${version})`,
        value: `${id}; // ${version}`,
        meta: 'crate',
        score: len - i, // Force alphabetic order before anything is typed
      }));
    }

    callback(null, suggestions);
  },
});

function useRafDebouncedFunction<A extends any[]>(fn: (...args: A) => void, onCall?: (...args: A) => void) {
  const timeout = useRef<number>();

  return useCallback((...args: A): void => {
    if (timeout.current) {
      window.cancelAnimationFrame(timeout.current);
    }

    timeout.current = window.requestAnimationFrame(() => {
      fn(...args);
      if (onCall) { onCall(...args); }
    });
  }, [fn, onCall, timeout]);
}


interface AdvancedEditorCoreProps extends CommonEditorProps {
  autocompleteOnUse: boolean;
  keybinding?: string;
  theme: string;
  resizeKey: AceResizeKey;
  pairCharacters: PairCharacters;
}

// Run an effect when the editor or prop changes
function useEditorProp<T>(editor: AceEditor, prop: T, whenPresent: (editor: AceEditor, prop: T) => void) {
  useEffect(() => {
    if (editor) {
      return whenPresent(editor, prop);
    }
  }, [editor, prop, whenPresent]);
}

const AdvancedEditorCore: React.SFC<AdvancedEditorCoreProps> = props => {
  const [editor, setEditor] = useState<AceEditor>(null);
  const child = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!child.current) { return; }

    const editor = ace.edit(child.current, {
      mode: 'ace/mode/rust',
    });
    setEditor(editor);

    // The default keybinding of control/command-l interferes with
    // the browser's "edit the location" keycommand which I think
    // is way more common.
    const gotoCommand = editor.commands.byName.gotoline;
    gotoCommand.bindKey = {
      win: 'Ctrl-Shift-L',
      mac: 'Command-Shift-L',
    };
    editor.commands.addCommand(gotoCommand);

    editor.setOptions({
      enableBasicAutocompletion: true,
      fixedWidthGutter: true,
    });

    const danglingElement = child.current;

    return () => {
      editor.destroy();
      setEditor(null);
      danglingElement.textContent = '';
    };
  }, [child]);

  useEditorProp(editor, props.execute, useCallback((editor, execute) => {
    // TODO: Remove command?
    editor.commands.addCommand({
      name: 'executeCode',
      bindKey: {
        win: 'Ctrl-Enter',
        mac: 'Ctrl-Enter|Command-Enter',
      },
      exec: execute,
      readOnly: true,
    });
  }, []));

  const autocompleteProps = useMemo(() => ({
    autocompleteOnUse: props.autocompleteOnUse,
    crates: props.crates,
  }), [props.autocompleteOnUse, props.crates]);

  // When the user types either `extern crate ` or `use `, automatically
  // open the autocomplete. This should help people understand that
  // there are crates available.
  useEditorProp(editor, autocompleteProps, useCallback((editor, { autocompleteOnUse, crates }) => {
    editor.commands.on('afterExec', ({ editor, command }) => {
      if (!(command.name === 'backspace' || command.name === 'insertstring')) {
        return;
      }

      if (displayExternCrateAutocomplete(editor, autocompleteOnUse)) {
        editor.execCommand('startAutocomplete');
      }
    });

    editor.completers = [buildCrateAutocompleter(autocompleteOnUse, crates)];
  }, []));

  // Both Ace and the playground want to be the One True Owner of
  // the textual content. This can cause issues because the Redux
  // store will attempt to change Ace in response to changes
  // *originating* from Ace. In addition, Ace can generate multiple
  // `change` events in response to what looks like a single user
  // action. This includes:
  //
  // - Auto-indenting after pressing return
  // - Invoking undo
  // - Multi-cursor editing
  //
  // To avoid issues...
  //
  // 1. When we are setting the Ace value based on the prop, we
  //    prevent generating outgoing events. This requires that the
  //    events are synchronously generated during the call to
  //    `setValue`
  //
  // 2. We throttle outgoing events to once per animation frame,
  //    only sending the most recent update. This reduces the updates
  //    to Redux and thus the number of updates to our props. While
  //    this covers a lot of the problems, it does not handle rapid
  //    typing (a.k.a. banging on the keyboard).
  //
  // 3. When we do generate an outgoing event, we log it. If we see
  //    that same event come back next via the property, we ignore it.
  //
  // 4. When all else fails, we ignore the prop if the value to set is
  //    what Ace already has.
  const doingSetProp = useRef(false);
  const previouslyNotified = useRef([]);
  const onEditCodeDebounced = useRafDebouncedFunction(
    props.onEditCode,
    useCallback(code => previouslyNotified.current.push(code), [previouslyNotified]),
  );

  useEditorProp(editor, onEditCodeDebounced, useCallback((editor, onEditCode) => {
    const listener = editor.on('change', _delta => {
      if (!doingSetProp.current) {
        onEditCode(editor.getValue());
      }
    });

    return () => {
      editor.off('change', listener);
    };
  }, []));

  useEditorProp(editor, props.code, useCallback((editor, code) => {
    // Is this prop update the result of our own `change` event?
    const last = previouslyNotified.current.shift();
    if (code === last) {
      return;
    }

    // It wasn't; discard any remaining self-generated events and resync
    previouslyNotified.current = [];

    // Avoid spuriously resetting the text
    if (editor.getValue() === code) {
      return;
    }

    doingSetProp.current = true;
    const currentSelection = editor.selection.toJSON();
    editor.setValue(code);
    editor.selection.fromJSON(currentSelection);
    doingSetProp.current = false;
  }, []));

  useEditorProp(editor, props.theme, useCallback((editor, theme) => {
    editor.setTheme(`ace/theme/${theme}`);
  }, []));

  const keybindingProps = useMemo(() => ({
    keybinding: props.keybinding,
  }), [props.keybinding]);

  useEditorProp(editor, keybindingProps, useCallback((editor, { keybinding }) => {
    const handler = keybinding ? `ace/keyboard/${keybinding}` : null;
    editor.setKeyboardHandler(handler);

    if (keybinding === 'vim') {
      const { CodeMirror: { Vim } } = ace.require('ace/keyboard/vim');
      Vim.defineEx('write', 'w', (cm, _input) => {
        cm.ace.execCommand('executeCode');
      });
    }
  }, []));

  useEditorProp(editor, props.pairCharacters, useCallback((editor, pairCharacters) => {
    editor.setBehavioursEnabled(pairCharacters !== PairCharacters.Disabled);
  }, []));

  useEditorProp(editor, props.position, useCallback((editor, { line, column }) => {
    // Columns are zero-indexed in ACE
    editor.gotoLine(line, column - 1, false);
    editor.focus();
  }, []));

  const selectionProps = useMemo(() => ({
    selection: props.selection,
  }), [props.selection]);

  useEditorProp(editor, selectionProps, useCallback((editor, { selection }) => {
    if (selection.start && selection.end) {
      // Columns are zero-indexed in ACE, but why does the selection
      // API and `gotoLine` treat the row/line differently?
      const toPoint = ({ line, column }: Position) => ({ row: line - 1, column: column - 1 });

      const start = toPoint(selection.start);
      const end = toPoint(selection.end);

      const range = new ace.Range(start.row, start.column, end.row, end.column);

      editor.selection.setRange(range);
      editor.renderer.scrollCursorIntoView(start);
      editor.focus();
    }
  }, []));

  // There's a tricky bug with Ace:
  //
  // 1. Open the page
  // 2. Fill up the page with text but do not cause scrolling
  // 3. Run the code (causing the pane to cover some of the text)
  // 4. Try to scroll
  //
  // Ace doesn't know that we changed the visible area and so
  // doesn't recalculate. We track factors that lead to this case to
  // force such a recalculation.
  useEditorProp(editor, props.resizeKey, useCallback((editor, _resizeKey) => {
    editor.resize();
  }, []));

  return (
    <div className={styles.advanced} ref={child} />
  );
};

export default AdvancedEditorCore;
