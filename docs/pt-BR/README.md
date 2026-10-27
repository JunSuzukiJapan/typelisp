<!-- translated-from: docs/ja/README.md @ 804fb3914b90de67225569295b86c07e97db869d -->
# Documentação do typelisp (português do Brasil)

typelisp é um Lisp com tipagem estática. Para instalá-lo e compilá-lo, consulte o
[README.md](../../README.md) (em inglês) na raiz do repositório.

## Recursos

### Tarefas e canais

Inicie tarefas (threads leves) e passe valores entre elas por meio de canais. Consulte
[Concorrência](tutorial/concurrency.md) para mais detalhes.

```lisp
(use std::sync::Chan)
(defun worker ((name string) (jobs Chan<int>) (results Chan<string>)) ()
  (doiter (n jobs)
    (sleep 0.1)
    (send results (format false "~a: ~d * ~d = ~d" name n n (* n n)))))

(defun main () ()
  (let ((jobs (the Chan<int> (Chan::new 0)))
        (results (the Chan<string> (Chan::new 10))))
    (task (worker "A" jobs results))
    (task (worker "B" jobs results))
    (dotimes (i 4)
      (send jobs (+ i 1)))
    (close jobs)
    (dotimes (i 4)
      (println "~a" (unwrap (recv results))))))

(main)
```

### Option e Result em vez de nil

Não existe `nil`. Uma função que pode não ter valor retorna `Option<T>`, e uma que pode falhar
retorna `Result<T,E>`; o conteúdo é extraído com `match`. Usar um `Option<string>` como `string`, ou
deixar de fora o caso `none` em um `match`, é um erro de tipo antes de o programa ser executado.
Consulte [Fundamentos de tipos](tutorial/types.md) e [Tratamento de erros](tutorial/errors.md) para
mais detalhes.

```lisp
(use std::num::parse-int)
(defun find-user ((id int)) Option<string>
  (case id
    (1 (Option::some "alice"))
    (2 (Option::some "bob"))
    (:else (Option::none))))

(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (<= 1 n 65535)
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))

(match (find-user 3)
  ((some name) (println "user: ~a" name))
  ((none) (println "not found")))        ; not found

(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))     ; error: port out of range: 99999
```

### Macros com verificação de tipos

As macros são escritas com `defmacro`, quasiquote e `gensym`, como em Common Lisp. O código
expandido é verificado como qualquer código escrito à mão e, se não se encaixar nos tipos, o erro é
apontado no local onde a macro foi chamada. Consulte [Macros](tutorial/macros.md) para mais
detalhes.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))

(let ((x 1) (y 2))
  (swap x y)
  (println "x=~a y=~a" x y))             ; x=2 y=1
```

```lisp
(defmacro add-a (x) `(+ ,x "a"))
(add-a 1)
;; error: type error: type mismatch: expected `int`, found `string`
```

## Tutorial

Se este é seu primeiro contato com o typelisp, leia nesta ordem.

- [Primeiros passos](tutorial/intro.md): o REPL, funções, variáveis, condicionais, laços, listas e `Vector`
- [Fundamentos de tipos](tutorial/types.md): tipos estáticos, `Option`, `Result`, estruturas, enumerações, genéricos
- [Traits](tutorial/traits.md): `deftrait` / `impl`, restrições de trait, `:dyn`
- [Macros](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [Tratamento de erros](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Concorrência](tutorial/concurrency.md): tarefas, canais, `select`, `Mutex`, `thread`

## Guias

- [Módulos e organização de arquivos](guide/modules.md): `use`, `pub`, como os arquivos correspondem aos módulos
- [Compilação](guide/compile.md): o JIT, criar executáveis com compilação AOT, dumps
- [E/S de arquivos, streams e rede](guide/io.md): arquivos, nomes de caminho, TCP / TLS / UDP, resolução de nomes
- [FFI de C](guide/ffi.md): chamar funções C com `defffi` (incluindo callbacks e structs C com `def-c-struct`)
- [Integração com editores](guide/editors.md): `typl-lsp` e a configuração do VS Code / Emacs
- [Para programadores de Common Lisp](guide/from-common-lisp.md): em que o typelisp difere do CL e como reescrever código CL

## Referência

- [Referência de sintaxe](reference/syntax.md): léxico, escrita de tipos, definições, formas de controle, compilação, concorrência
- [Funções embutidas](reference/functions/README.md): funções embutidas, métodos e a biblioteca padrão
- [Tipos](reference/types.md): os tipos e os traits que cada um implementa
- [Mensagens de erro](reference/errors.md): o que significam os erros mais comuns e como corrigi-los

## Integração com editores

Os passos de configuração estão no [guia de integração com editores](guide/editors.md). Os atalhos de
teclado e as configurações de cada editor estão descritos nestes documentos:

- [Emacs (typelisp-mode)](../../editor/emacs/README_pt-BR.md)
- [VS Code](../../editor/vscode/README_pt-BR.md)
