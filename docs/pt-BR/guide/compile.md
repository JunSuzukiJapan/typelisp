<!-- translated-from: docs/ja/guide/compile.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# Compilação

Se você não fizer nada além disso, os programas typelisp são executados no interpretador. Além disso há duas
formas de compilar para código nativo e uma forma de salvar um ambiente. Os detalhes da especificação estão
no [capítulo 10 da Referência de sintaxe](../reference/syntax.md#10-compilação).

| Método | Como | Resultado |
|---|---|---|
| Compilação JIT | `(compile name)` | Uma função da sessão em andamento é substituída por código nativo |
| Compilação AOT | `typl -c src.typl` ou `(compile-file "src.typl" "out")` | Um executável independente |
| Dump | `(dump "file.typld")` | Salva as definições; `typl --image` reinicia a partir do mesmo ambiente |

## 1. Preparação

A compilação usa o LLVM 22. Se você compilou o `typl` seguindo o [README.md](../../../README.md), não é preciso
mais nenhuma preparação.

Os executáveis criados por compilação AOT são ligados à biblioteca estática `libtypelisp_front.a`. Uma build de
release do `typl` (incluindo uma instalada com `cargo install`) carrega essa biblioteca dentro de si, então não
é preciso preparar nada. Na primeira vez que compila, ele grava a biblioteca em
`~/.typelisp/lib/<ID da build>/` (ou em `$TYPELISP_HOME/lib/<ID da build>/` se a variável de ambiente
`TYPELISP_HOME` estiver definida) e usa essa cópia daí em diante. `typl --remove-lib` a apaga (com `--others`,
as gravadas por outras versões do `typl`; com `--all`, todas). Uma build de depuração do `typl` usa a
biblioteca em `target/debug/` do repositório em que foi compilado. Para usar uma colocada em outro lugar,
informe a pasta com `--lib-dir` ao iniciar o `typl` (seção 3.2).
No macOS, a ligação usa as Xcode Command Line Tools.

## 2. Compilação JIT

Transforma na hora uma função já definida em código nativo.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; daqui em diante, as chamadas executam o código compilado
```

- `name` não é avaliado. Escreva o nome da função como está (não como string). Para um método, escreva-o com
  o nome do tipo, como em `(compile point::norm)`.
- As funções que ela chama são compiladas junto.
- **Funções genéricas não podem ser compiladas.** Em cada lugar onde são usadas é criada uma cópia para cada
  tipo. Compile em vez disso a função que a chama com tipos concretos.
- `trace`, `step`, `disassemble`, `compile`, `compile-file` e `dump` são operações do interpretador, então uma
  função que as chama não pode ser compilada. Tentar compilá-la dá um erro que explica o motivo.

Para ver o resultado da compilação, use `disassemble`.

```lisp
(disassemble fib)          ; o código de máquina do host
(disassemble fib true)     ; LLVM IR
```

## 3. Construir um executável com compilação AOT

### 3.1 Escrever o programa

Como ponto de entrada, defina uma **função `main` que não recebe argumentos**.

```lisp
;; hello.typl
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(defun main () ()
  (println "args: ~a" (command-line-args))
  (println "fib(25) = ~a" (fib 25)))

(main)
```

O `(main)` no final do arquivo está lá para que `main` seja chamada quando você executa `typl hello.typl`.
`compile-file` pula esse `(main)` final, então o mesmo arquivo funciona tanto no interpretador quanto com a
compilação AOT.

### 3.2 Compilar

Na linha de comando, use `typl -c` (`typl --compile` é o mesmo).

```sh
$ typl -c hello.typl            # cria hello
$ typl -c hello.typl -o fib     # chama o executável de fib
$ ./hello a b
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

Sem `-o`, o executável recebe o nome do arquivo-fonte sem `.typl` e é colocado na mesma pasta do
arquivo-fonte. Se o nome do arquivo-fonte não terminar em `.typl`, `-o` é obrigatório. Com `-c`
(`--compile`), não é possível passar `--image`, `--heap-cells` nem `--feature`.

Você pode fazer o mesmo chamando `compile-file` no REPL ou em um programa.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

Se você compila com frequência, pode pôr esta linha em um arquivo e executá-lo com `typl build.typl`.

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

Os nomes de arquivo são resolvidos a partir do **diretório atual em que o `typl` foi iniciado**, não da
localização de `build.typl`.

Para ligar um `libtypelisp_front.a` colocado em um lugar diferente de onde o `typl` procura, informe a pasta
com `--lib-dir`. Vale tanto para `typl -c` quanto para `compile-file`.

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

Se a pasta informada não tiver `libtypelisp_front.a`, o `typl` para com um erro. O arquivo só funciona com o
`typl` compilado junto com ele. Depois de recompilar o `typl`, copie-o de novo.

### 3.3 O que um arquivo compilado com AOT pode conter

- O nível superior do arquivo de entrada só pode conter definições (`defun` `defmethod` `defvar`
  `defparameter` `defconstant` `defmacro` `defsignature` `defstruct` `defenum` `deftype` `deftrait`
  `impl` `defffi`, `(unsafe (def-c-struct ...))`) e `use` `module`. Expressões de nível superior como
  `(println ...)` não são permitidas, exceto o `(main)` final. Coloque o trabalho dentro de `main`.
- Sem uma `main` que não recebe argumentos, a compilação falha com um erro.
- Os arquivos dos módulos usados também são compilados e combinados em um único executável.
- As bibliotecas indicadas com `:library` em `defffi` são ligadas automaticamente ([FFI de C](ffi.md)).
- Todas as funções da biblioteca padrão podem ser usadas com a compilação AOT. `eval` também pode ser usado,
  mas então o verificador de tipos e o interpretador entram no executável, que fica maior e mais lento para
  iniciar. Programas que não chamam `eval` não os incluem.

### 3.4 Como o executável se comporta

- `(command-line-args)` devolve um `Vector<string>` com a mesma forma, seja executado como
  `typl hello.typl a b` ou como `./hello a b`. O primeiro elemento é o nome do programa.
- O código de saída é definido com `(exit n)`. Se `main` retornar normalmente, ele é 0.
- Em um `panic`, o programa imprime a mensagem e sai com um código diferente de zero.

## 4. Dumps

Você pode salvar as definições da sessão atual em um arquivo e começar a partir dele na próxima vez.

```sh
$ typl
typl> (defun sq ((n i32)) i32 (* n n))
typl> (compile sq)
true
typl> (dump "session.typld")
true
typl> :quit
$ typl --image session.typld
typl> (sq 9)
81
```

Também funciona para executar um arquivo, como em `typl --image session.typld prog.typl`.

- O que é salvo são as **definições**. As expressões avaliadas na sessão não são salvas.
- As funções que você compilou com `compile` são salvas na forma compilada.
- As variáveis globais são restauradas **executando de novo seus inicializadores**, não com os valores que
  tinham quando o dump foi gravado.
- Um dump não pode ser carregado por um `typl` de versão diferente da que o gravou (é um erro).

Se você executar um arquivo e fizer `(dump ...)` a partir dele, as definições desse arquivo ficam em um módulo
com o nome do arquivo. Uma função definida em `dp.typl` se chama `dp::sq`, e chamá-la de outro arquivo exige
`pub` ([Módulos e organização de arquivos](modules.md)).

## 5. Sobre arquivos de módulos compilados

Não existe um formato, como o `.fasl` do Common Lisp, para gravar em arquivo o resultado compilado de cada
módulo. `compile-file` constrói o executável diretamente a partir dos fontes. Não sobram arquivos
intermediários.
