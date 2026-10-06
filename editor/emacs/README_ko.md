<!-- translated-from: editor/emacs/README_JP.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# typelisp-mode (Emacs)

typelisp 소스(`.typl`)를 편집하기 위한 Emacs 메이저 모드입니다.
VS Code 판은 [../vscode/](../vscode/README_ko.md)에 있습니다. 둘은 같은 키워드 표와 같은 들여쓰기
규칙을 가지며, 이는 `cargo test --test editor_keyword_sync_test`가 기계적으로 검사합니다(끝부분 참고).

## 기능

- 구문 강조
  - 특수 형식 / 제어 구문(`defun` `let` `if` `match` `loop` `lambda` `setf` `as` `apply`,
    `print`/`println`/`format`, `pprint` 계열 등)
  - 정의 이름 강조(`(defun NAME ...)`의 `NAME`은 함수 이름, `(defstruct NAME ...)`은 타입 이름,
    `(defvar (NAME ...))`은 변수 이름으로. `pub`가 붙은 `(pub defun NAME ...)`도 마찬가지)
  - 이름공간·선언 키워드(`pub` `module` `use` `load` `impl` `where`)와
    람다 리스트 표지(`&rest` `&optional` `&key`)
  - 내장 함수(`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car` 등)
  - 기본 타입(`bignum` / `ratio` 포함)·내장 타입·내장 오류 타입(`ParseIntError` 등)·
    `Capitalized`인 사용자 타입·트레이트 객체 타입 `:dyn Trait`
  - **사용자 정의 타입의 사용 위치**(`defstruct`/`defenum`/`deftrait`의 이름은 보통 소문자
    (`rect` `todo-item` `board`)라서 `Capitalized` 규칙으로는 잡히지 않습니다).
    `typl-lsp`에 연결되어 있으면 서버의 semantic tokens로 색을 칠합니다(`eglot`에서도 동작합니다.
    뒤에서 설명). 연결되지 않았을 때는 버퍼 안의 타입 이름을 모으는 대체 방식으로 전환됩니다
  - 리터럴(`true` `false`, 숫자 리터럴(10진 / `0xff` / `1.5` / `1/3`),
    문자 리터럴 `#\Space`, 문자열, 키워드 `:name`)
  - 문자열 안의 `format` 제어 지시자(`~a` `~5,'0d` `~{...~}` 등)
  - CL 방식의 earmuff가 붙은 전역 변수(`*print-pretty*` 등)
- 주석
  - 행 주석 `;`
  - **중첩 가능한** 블록 주석 `#| ... |#`
- S 식 탐색과 Lisp 방식 들여쓰기
- `imenu`를 통한 정의 목록(함수 / 메서드 / 매크로 / 타입 / 트레이트 / `impl` / 변수 / 모듈)
- `typl` CLI 호출(아래)

## 키 바인딩

| 키 | 명령 | 내용 |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | 저장하고 `typl FILE`로 실행(`compile`을 거치므로 오류 행으로 이동 가능) |
| `C-c C-z` | `typelisp-repl` | comint 버퍼에서 `typl`의 REPL을 시작 |

`typl`의 위치는 `typelisp-program`(기본값 `"typl"`)으로 지정합니다.
진단은 `error: FILE:LINE:COL: ...` 형식이라 `compilation-mode`가 해석할 수 있고,
`next-error` / `C-x \``로 바로 해당 위치로 이동할 수 있습니다.

## 설치

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

`.typl` 파일은 자동으로 `typelisp-mode`로 열립니다(`auto-mode-alist`에 등록되어 있습니다).

`use-package`를 쓰는 경우:

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## 언어 서버(`typl-lsp`)

`typl-lsp`를 빌드하면 `eglot`(Emacs 29+ 기본 포함)이나 `lsp-mode`에서 사용할 수 있습니다.

```sh
cargo build --release --bin typl-lsp
```

`eglot`의 경우:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

`lsp-mode`의 경우:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

진단(구문/타입 오류와 재정의 경고를 `textDocument/publishDiagnostics`로 통지)·hover·
정의로 이동(goto-definition)·완성(`:`를 트리거 문자로 등록)·semantic tokens를 지원합니다.
`use`를 통한 파일 간 참조도 해석됩니다(프로젝트 루트의 `typelisp.toml`을 위쪽으로 탐색. 자세한 내용은
[구문 레퍼런스 3.11](../../docs/ko/reference/syntax.md#311-파일과-모듈의-대응여러-파일-프로젝트)).
열려 있는 에디터 버퍼의 저장하지 않은 편집은 의존하는 파일과 의존되는 파일 양쪽의 진단에 즉시
반영됩니다.

### 타입 이름 강조(semantic tokens)

서버는 `textDocument/semanticTokens`로 **검사기가 실제로 타입 이름으로 해석한 위치**를 보고합니다.
텍스트 대조가 아니므로,

- `use`를 거쳐 다른 파일에서 온 타입에도 색이 칠해집니다(버퍼 안의 해석으로는 원리적으로 닿지 않는 범위)
- 타입과 같은 이름의 **함수** 호출 위치에는 색이 칠해지지 않습니다(그곳은 검사기가 함수로 해석했으므로
  애초에 토큰이 기록되지 않습니다)

클라이언트 쪽:

- **`eglot`(Emacs 31 이후)**: eglot이 직접 그립니다(`eglot-semantic-tokens-mode`).
  `typelisp-mode` 쪽은 관여하지 않습니다
- **`eglot`(Emacs 30 이전)**: 이 버전의 eglot은 semanticTokens를 다루지 않습니다. 그래서
  **`typelisp-mode`가 직접 요청을 보내고 오버레이로 그립니다**
  (`typelisp-semantic-tokens-mode`. eglot이 연결되면 자동으로 켜집니다)
- **`lsp-mode`**: 기본 지원(`lsp-semantic-tokens-enable`을 `t`로). 이 경우
  `typelisp-mode` 쪽은 관여하지 않습니다

`scripts/emacs-semantic-smoke.el`은 실제로 eglot으로 연결하여, 사용 중인 Emacs에서 그리기를 맡는 쪽을
검사합니다. 어느 클라이언트든 서버가 응답하는 동안에는 버퍼 안 해석의 대체 방식이 물러납니다
(같은 버퍼를 두 규칙이 칠하지 않도록 하기 위해서입니다).

| 설정 | 기본값 | 내용 |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | Emacs 30 이전의 eglot을 쓸 때 서버의 semantic tokens로 색을 칠할지 |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | 편집 후 다시 요청하기까지의 유휴 초 수(`eglot-send-changes-idle-time`보다 크게 할 것) |

## 비고

- typelisp는 심볼을 읽을 때 소문자로 바꾸지만, 강조는 대문자로 시작하는 타입 이름을 구별하기 위해
  대소문자를 구분합니다.
- 들여쓰기는 전용 `typelisp-indent-function`이 `typelisp-indent-specs`(연관 리스트)를 찾아
  결정합니다. Emacs Lisp와 이름을 공유하는 형식(`defun` `let` `if` …)도 직접 가지고 있는 이유는
  심볼 속성이 **전역**이라서, typelisp용 설정이 같은 세션의 다른 Lisp 버퍼의 들여쓰기를 바꿔 버리기
  때문입니다. 또 typelisp의 형식은 Emacs Lisp와 이름이 같아도 모양이 다릅니다——
  `(defun NAME (PARAMS) RETTYPE ...)`은 헤더 요소가 3개이고, `if`는 `else`가 필수인 3요소 고정입니다——
  그래서 값도 공유할 수 없습니다.
  `examples/` 아래의 모든 `.typl` 파일이 `indent-region`으로 1바이트도 바뀌지 않는다는 것과,
  들여쓰기를 전부 없앤 뒤 다시 들여쓰면 원래대로 돌아온다는 것을 확인했습니다(VS Code 판도 같은
  파일들에서 같은 기준을 만족합니다).

## 에디터 정의의 어긋남 검출

키워드 표는 VS Code 판과 이중으로 관리됩니다. 구현은 진행되었는데 에디터 정의만 낡는 사고를 막기
위해 Rust 쪽에 테스트가 있습니다:

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
