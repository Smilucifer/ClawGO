<script lang="ts">
  import { onMount } from "svelte";

  let {
    content = "",
    language = "plaintext",
    onChange = (_value: string) => {},
    theme = "vs-dark",
  }: {
    content: string;
    language: string;
    onChange?: (value: string) => void;
    theme?: string;
  } = $props();

  let containerRef: HTMLDivElement | undefined = $state();
  let editor: import("monaco-editor").editor.IStandaloneCodeEditor | undefined;
  let _monaco: typeof import("monaco-editor") | undefined;
  let disposed = false;
  let programmaticChange = false;

  onMount(() => {
    import("monaco-editor").then((monaco) => {
      _monaco = monaco;
      if (!containerRef || disposed) return;
      editor = monaco.editor.create(containerRef, {
        value: content,
        language,
        theme,
        minimap: { enabled: false },
        lineNumbers: "on",
        scrollBeyondLastLine: false,
        wordWrap: "on",
        automaticLayout: true,
        readOnly: false,
        fontSize: 13,
        fontFamily: "'Cascadia Code', 'Fira Code', 'JetBrains Mono', monospace",
        tabSize: 2,
        "semanticHighlighting.enabled": true,
        suggest: { showWords: false, showSnippets: false },
        quickSuggestions: false,
        parameterHints: { enabled: false },
      });

      editor.onDidChangeModelContent(() => {
        if (disposed) return;
        if (programmaticChange) {
          programmaticChange = false;
          return;
        }
        onChange(editor!.getValue());
      });
    });

    return () => {
      disposed = true;
      editor?.dispose();
    };
  });

  $effect(() => {
    if (editor && content !== editor.getValue()) {
      programmaticChange = true;
      editor.setValue(content);
    }
  });

  $effect(() => {
    // Read reactive props before bail so they are always tracked as dependencies
    const lang = language;
    const thm = theme;
    if (!editor || !_monaco) return;
    const m = _monaco;
    const model = editor.getModel();
    if (model) {
      const supported = m.languages.getLanguages().some((l) => l.id === lang);
      m.editor.setModelLanguage(model, supported ? lang : "plaintext");
    }
    editor.updateOptions({ theme: thm });
  });
</script>

<div bind:this={containerRef} class="h-full w-full"></div>
