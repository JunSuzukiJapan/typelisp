<!-- translated-from: editor/vscode/README_JP.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# typelisp (VS Code)

typelisp 소스(`.typl`)를 편집하기 위한 VS Code 확장입니다.
Emacs 판은 [../emacs/](../emacs/README_ko.md)에 있습니다. 둘은 같은 키워드 표와 같은 들여쓰기 규칙을
가지며, 이는 `cargo test --test editor_keyword_sync_test`가 기계적으로 검사합니다(뒤에서 설명).

## 기능

- **구문 강조**(TextMate 문법, 언어 서버 불필요)
  - 특수 형식 / 제어 구문(`defun` `let` `if` `match` `loop` `lambda` `setf` `as` `apply`,
    `print`/`println`/`format`, `pprint` 계열)
  - 정의 이름(`(defun NAME ...)`은 함수, `(defstruct NAME ...)`은 타입, `(defvar (NAME ...))`은 변수.
    `pub`가 붙은 `(pub defun NAME ...)`도 마찬가지)과 `(impl Trait Type)`의 두 이름
  - 이름공간·선언 키워드(`pub` `module` `use` `load` `impl` `where`),
    람다 리스트 표지(`&rest` `&optional` `&key`)
  - 내장 함수·기본 타입(`bignum` / `ratio` 포함)·내장 오류 타입·
    `Capitalized`인 사용자 타입·트레이트 객체 타입 `:dyn Trait`(제네릭 인수 안쪽도)
  - 숫자 리터럴(10진 / `0xff` / `1.5` / `3.0e10` / `1/3`), 문자 리터럴 `#\Space`,
    키워드 `:name`, earmuff가 붙은 전역 변수 `*print-pretty*`
  - **문자열 안의 `format` 제어 지시자**(`~a` `~5,'0d` `~{...~}` `~^` 등)
  - 행 주석 `;`과 **중첩 가능한** 블록 주석 `#| ... |#`
- **사용자 정의 타입의 사용 위치**(semantic tokens)
  - `defstruct` / `defenum` / `deftrait`의 이름은 보통 소문자(`rect` `todo-item` `board`)라서
    `Capitalized` 규칙으로는 잡히지 않고, TextMate 문법은 행 단위로 동작해 파일 전체를 볼 수 없습니다.
    semantic tokens는 볼 수 있으므로, 정적 타입 언어인데 타입 주석에만 색이 칠해지지 않던 상태를
    해소했습니다
  - `typl-lsp`에 연결되어 있으면 **검사기가 실제로 타입 이름으로 해석한 위치**가 돌아옵니다. 따라서
    `use`로 다른 파일에서 온 타입에도 색이 칠해지고, 타입과 같은 이름의 **함수** 호출 위치에는 색이
    칠해지지 않습니다(그곳은 검사기가 함수로 해석했으므로 애초에 토큰이 기록되지 않습니다)
  - 연결되지 않았거나 빌드되지 않았을 때는 확장이 파일 안에서 닫힌 채 해석하는 텍스트 탐색 대체 방식으로
    전환됩니다. 이쪽은 근사치로, 다른 파일에서 온 타입을 잡지 못하고 타입과 같은 이름의 함수도 구별하지
    못합니다
- **Lisp 들여쓰기**(VS Code는 Lisp 들여쓰기를 기본으로 갖고 있지 않으므로 확장에서 구현)
  - 문서 서식·선택 영역 서식·입력 시 서식(Enter와 `)`, `editor.formatOnType`이 켜져 있을 때)
- **Outline / breadcrumbs / `Ctrl+Shift+O`**(함수·메서드·매크로·타입·트레이트·
  `impl`·변수·모듈)
- **`typl-lsp` 연동**(진단·hover·정의로 이동·완성·semantic tokens)
- **`typl` CLI 명령**(실행·REPL)

언어 서버 이외에는 모두 확장 단독으로 동작하므로, `typl-lsp`를 빌드하지 않은 체크아웃에서도
강조·들여쓰기·Outline·(파일 안에 닫힌) 타입 강조를 쓸 수 있습니다.

## 설치

Marketplace에는 올리지 않았으므로 로컬에서 빌드해 설치합니다.

```sh
cd editor/vscode
npm install
npm run compile
```

그런 다음 다음 중 하나:

- **개발 호스트에서 시험**: `editor/vscode`를 VS Code로 열고 `F5`
- **영구 설치**: `npx @vscode/vsce package`로 `.vsix`를 만들고,
  확장 뷰의 "…" → "Install from VSIX..."

`.typl` 파일은 자동으로 typelisp 모드로 열립니다.

## 키 바인딩

| 키 | 명령 | 내용 |
|---|---|---|
| `Ctrl+Alt+R` | `typelisp.runFile` | 저장하고 `typl FILE`로 실행 |
| `Ctrl+Alt+Z` | `typelisp.repl` | `typl`의 REPL을 시작 |

명령 팔레트에는 `typelisp: Restart Language Server`도 있습니다.

## 설정

| 설정 | 기본값 | 내용 |
|---|---|---|
| `typelisp.program` | `typl` | `typl` CLI의 경로 |
| `typelisp.languageServer.enable` | `true` | `typl-lsp`에 연결할지 |
| `typelisp.languageServer.path` | (비어 있음) | `typl-lsp`의 경로. 비어 있으면 워크스페이스의 `target/release/typl-lsp` → `target/debug/typl-lsp` → `PATH` 순서로 찾음 |
| `typelisp.trace.server` | `off` | LSP의 JSON-RPC를 기록 |

언어 서버는 다음으로 빌드합니다:

```sh
cargo build --release --bin typl-lsp
```

`use`를 통한 파일 간 참조는 프로젝트 루트의 `typelisp.toml`을 위쪽으로 탐색해 해석됩니다
(자세한 내용은 [구문 레퍼런스 3.11](../../docs/ko/reference/syntax.md#311-파일과-모듈의-대응여러-파일-프로젝트)).

## 작업의 problem matcher

`typelisp`라는 problem matcher를 제공합니다. `typl`의 진단은
`error: FILE:LINE:COL: message` 형식이므로 그대로 Problems 패널에 낼 수 있습니다:

```jsonc
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "typl: run",
      "type": "shell",
      "command": "typl ${file}",
      "problemMatcher": "$typelisp"
    }
  ]
}
```

## 개발

```sh
npm run compile   # tsc
npm run watch     # 감시 빌드
npm test          # node --test(문법·들여쓰기·심볼·타입 참조·매니페스트)
```

테스트는 `vscode` 모듈이 필요 없는 부분만 대상으로 합니다. 그래서 `src/indent.ts`와
`src/symbols.ts`는 순수 함수로 작성되어 있고, `src/extension.ts`만 에디터 API와 닿습니다.

- `src/test/grammar.test.ts` — VS Code와 같은 엔진(`vscode-textmate` +
  `vscode-oniguruma`)으로 문법을 실제로 토큰화해 검사합니다.
  Oniguruma는 Emacs 정규 표현식과 세부가 다르므로(예: 문자 클래스 맨 앞의 `]`를 리터럴로 취급하지
  않음), 이런 차이는 실제 엔진으로 밟아 보지 않으면 발견되지 않습니다.
- `src/test/indent.test.ts` — `examples/`의 모든 `.typl` 파일에 대해,
  **들여쓰기를 전부 없앤 뒤 복원한 결과가 커밋된 내용과 바이트 단위로 일치할 것**을 요구합니다.
  Emacs 모드도 같은 파일들에서 같은 기준을 만족하며, 이것이 "두 에디터가 일치한다"를 검증된 주장으로
  만들고 있습니다.
  더해서 `src/test/fixtures/emacs-indent-reference.txt`는 Emacs의 `typelisp-mode` 버퍼에서
  `indent-region`을 실제로 실행해 채취한 참조 출력입니다. 기대값이 TS 구현의 추인이 아니라
  **다른 쪽 에디터가 실제로 내는 결과**이므로, 이식의 충실도가 그대로 검증됩니다
  (`let*` `do` `doiter` `labels` `impl` `pprint-logical-block`, quote 접두사 등을 포함).
- `src/test/symbols.test.ts` — Outline의 내용과 대체 방식의 타입 참조 검출. 정의 수는 행 머리의
  정의 형식을 세는 독립적인 방법과 완전히 일치할 것을 요구합니다. 타입 참조의 경계 규칙은 Emacs 판의
  대체 방식과 의도적으로 맞춰 두었습니다(VS Code는 lookbehind, Emacs는 앞 문자를 하나 소비하는
  형태로 같은 집합을 표현).
- 서버 쪽의 해석 기반 토큰(`crates/typelisp-front/src/check/semantic.rs`)은
  `cargo test --test lsp_semantic_test`와 `scripts/lsp-semantic-smoke.py`
  (실제 프로세스를 stdio로 구동)가 검사합니다. Emacs 쪽 클라이언트는
  `scripts/emacs-semantic-smoke.el`이 실제 eglot 연결로 검사합니다.
- `src/test/manifest.test.ts` — `package.json`은 컴파일러가 검사하지 않는 유일한 부분이므로,
  선언된 명령과 `registerCommand`의 집합 일치, 키 바인딩의 참조 대상, 코드가 읽는 설정이 선언되어
  있는지, problem matcher가 `typl`의 실제 출력을 해석할 수 있는지를 검사합니다.

### 에디터 정의의 어긋남 검출

키워드 표는 Emacs 판과 VS Code 판에서 이중으로 관리됩니다. 구현은 진행되었는데 에디터 정의만
낡는 사고를 막기 위해 Rust 쪽에 테스트가 있습니다:

```sh
cargo test --test editor_keyword_sync_test
```

prelude를 실제로 로드해 레지스트리를 훑고, **어느 한쪽 에디터가 모르는 이름**을 보고합니다.
특수 형식은 실행 시 표현을 가지지 않으므로 `crates/typelisp-front/src/check/checker.rs`의
`// SPECIAL-FORM DISPATCH BEGIN` / `END` 사이에서 읽어 냅니다(이 주석은 지우지 마십시오).
실패하면 보고된 이름을 **양쪽** 에디터 정의에 추가합니다.

같은 테스트가 semantic tokens의 legend도 대조합니다(`src/bin/lsp.rs`의 `SEMANTIC_TOKEN_TYPES`와
두 에디터가 가진 대응표가 이름·순서 모두 일치할 것). 어긋나도 실행 시 오류는 나지 않고 모든 토큰의
색이 뒤바뀔 뿐이므로, 기계적으로 고정해 두었습니다.

## 비고

- typelisp는 심볼을 읽을 때 소문자로 바꾸지만, 강조는 대문자로 시작하는 타입 이름을 구별하기 위해
  대소문자를 구분합니다.
- 들여쓰기는 `src/indent.ts`의 `INDENT_SPECS`가 결정합니다. Emacs 판
  `typelisp-indent-specs`의 이식으로, 값도 규칙도 같습니다. `(defun NAME (PARAMS) RETTYPE ...)`의
  헤더가 3요소라는 점, `if`가 `else` 필수인 3요소 고정이라는 점 등, Emacs Lisp와 이름이 같아도
  모양이 다른 점이 그대로 반영되어 있습니다.
- `#| ... |#`의 내용은 서식을 적용할 때 다시 들여쓰기됩니다. Emacs의 `indent-region`과 같은 동작으로
  맞춰 두었습니다.
