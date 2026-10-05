<!-- translated-from: docs/ja/guide/editors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 에디터 연동 (typl-lsp)

`typl-lsp`는 typelisp의 언어 서버이다. LSP(Language Server Protocol)를 지원하는 에디터에 연결하면 편집 중인 파일에
다음 기능을 제공한다.

- 진단: 읽기 오류, 타입 오류, 재정의 경고
- 호버: 괄호로 감싼 식의 타입과 그것이 호출하는 정의의 문서 문자열(맨 변수 이름에는 표시되지 않는다)
- 정의로 이동
- 완성(`:`을 입력하면 후보가 나온다)
- 타입 이름의 색칠(시맨틱 토큰). 다른 파일에서 `use`한 타입도 포함한다

`use`를 통한 파일 사이의 참조는 해석된다. 열려 있는 다른 파일의 저장하지 않은 편집도 그것을 `use`하는 파일의 진단에
바로 반영된다.

## 1. 빌드

```sh
cargo build --release --bin typl-lsp
```

`target/release/typl-lsp`가 만들어진다. [README.md](../../../README.md)에 따라 `cargo install`로 설치했다면 `typl`과
함께 `~/.cargo/bin/typl-lsp`에 있다.

## 2. VS Code

확장은 저장소의 `editor/vscode`에 있다. Marketplace에는 공개되어 있지 않으므로 직접 빌드해서 설치한다.

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # .vsix가 만들어진다
```

확장 보기의 "..." 메뉴에서 "Install from VSIX..."를 고르고 빌드한 `.vsix`를 선택한다.

확장은 작업 영역의 `target/release/typl-lsp`, `target/debug/typl-lsp`, `PATH` 순서로 `typl-lsp`를 찾는다. 다른 곳에
두었다면 설정 `typelisp.languageServer.path`에 그 경로를 쓴다.

| 설정 | 기본값 | 의미 |
|---|---|---|
| `typelisp.program` | `typl` | `typl`의 경로 |
| `typelisp.languageServer.enable` | `true` | `typl-lsp`에 연결할지 여부 |
| `typelisp.languageServer.path` | (비어 있음) | `typl-lsp`의 경로 |

`Ctrl+Alt+R`은 편집 중인 파일을 저장하고 `typl`로 실행하며, `Ctrl+Alt+Z`는 REPL을 시작한다. 자세한 내용은
[VS Code 확장의 README](../../../editor/vscode/README.md)(일본어)를 참고한다.

## 3. Emacs

`typelisp-mode`는 저장소의 `editor/emacs`에 있다.

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

`eglot`(Emacs 29 이후에 포함)로 `typl-lsp`에 연결하는 설정:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

`lsp-mode`를 쓰는 경우:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

eglot은 시맨틱 토큰을 지원하지 않으므로 eglot을 쓸 때는 `typelisp-mode`가 대신 타입 이름을 색칠한다. `lsp-mode`에서는
`lsp-semantic-tokens-enable`을 `t`로 한다.

`C-c C-c`는 편집 중인 파일을 실행하고, `C-c C-z`는 REPL을 시작한다. 자세한 내용은
[typelisp-mode의 README](../../../editor/emacs/README.md)(일본어)를 참고한다.

## 4. 그 밖의 에디터

`typl-lsp`는 표준 입출력으로 LSP를 주고받으며 명령줄 인수를 받지 않는다. 에디터의 LSP 클라이언트에서 `.typl` 파일에
대해 `typl-lsp`를 시작하도록 설정한다.

## 5. 프로젝트를 인식하는 방법

`typl-lsp`는 연 파일의 디렉터리에서 시작해 위로 올라가며 `typelisp.toml`을 찾고, 그곳을 소스 루트로 삼아 `use`를
해석한다. `typl`이 파일을 실행할 때와 같은 규칙이다([모듈과 파일 구성](modules.md#2-프로젝트-준비하기)). 여러 파일로
된 프로젝트에서는 그 루트에 `typelisp.toml`을 둔다.

## 6. 언어 서버는 프로그램을 실행하지 않는다

`typl-lsp`는 읽기와 타입 검사만으로 진단을 만든다. 편집 중인 프로그램을 실행하는 일은 없다. 진단은 키를 누를 때마다
돌기 때문에 거기서 부작용이 있는 코드나 끝나지 않는 코드를 실행할 수는 없다. 유일한 예외는 `defmacro`의 등록으로,
그 뒤에 오는 매크로 호출을 검사하는 데 필요하다.

그래서 `typl`이 프로그램을 실행했을 때만 일어나는 오류(`panic`, 파일이 없는 경우 등)는 언어 서버의 진단에 나타나지
않는다.
