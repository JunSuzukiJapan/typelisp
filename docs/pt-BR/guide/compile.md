<!-- translated-from: docs/ja/guide/compile.md @ 3a77c293e452d2d74bccfa67efd8af1f6f650f7d -->
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
(use std::env::command-line-args)
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
args: #(./hello a b)
fib(25) = 75025
```

Sem `-o`, o executável recebe o nome do arquivo-fonte sem `.typl` e é colocado na mesma pasta do
arquivo-fonte. Se o nome do arquivo-fonte não terminar em `.typl`, `-o` é obrigatório. Com `-c`
(`--compile`), não é possível passar `--image` nem `--feature`.

Com `--release`, o `typl` faz um **build de release**, que remove `eprint`/`eprintln` (saída de erro padrão para investigação; [Impressão](../reference/functions/printing.md)) junto com seus argumentos. O que ele remove continua sendo verificado por tipos, então um erro de tipo ali faz a compilação falhar. `--release` é uma opção do `typl` como um todo: funciona do mesmo jeito não só com `-c`, mas também ao executar um arquivo, no REPL e no `typl test`. Um `compile-file` chamado dentro de um `typl` iniciado com `--release` também faz um build de release, e `eprint`/`eprintln` também são removidos do código avaliado com `eval` dentro do executável feito assim.

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # o mesmo se escrito depois de -c
```

`-O0`, `-O1`, `-O2` e `-O3` definem o **nível de otimização**. O padrão é `-O0`, que não otimiza. A partir de `-O1`, o compilador faz primeiro otimizações simples, como substituir uma variável ligada apenas a um literal pelo seu valor ou calcular de antemão a aritmética inteira sobre constantes, e deixa o resto para a otimização do LLVM no nível de mesmo nome. Em todos os níveis os resultados do cálculo são os mesmos. Assim como `--release`, é uma opção do `typl` como um todo: além do executável que `-c` cria, vale para o código compilado ao executar um arquivo, no REPL e em `typl test`, e para um `compile-file` chamado ali. O código que o interpretador executa não é otimizado.

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # o mesmo se escrito depois de -c
```

Você pode fazer o mesmo chamando `compile-file` no REPL ou em um programa.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
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

- O nível superior do arquivo de entrada só pode conter definições (`defun` `defvar`
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

### 3.5 Recursos que o executável usa

`typl -c` tem opções que dizem quais recursos o executável pode usar enquanto roda.

```sh
$ typl --heap-cells 100000 -c hello.typl    # fixa a arena de cons em 100000 células
$ typl -c hello.typl --max-memory 64M       # aloca no máximo 64 MiB
$ typl -c hello.typl --no-cons              # não usa células cons
```

- `--heap-cells N` fixa a capacidade da arena de células cons em `N` células. A arena não cresce além disso: se a coleta de lixo não conseguir liberar uma célula, o programa para com um erro de execução. Sem a opção, a arena começa com 65536 células e cresce conforme necessário.
- `--max-memory SIZE` limita a memória total que o executável aloca. Tudo conta: a arena de cons, strings, valores como vetores e tabelas hash, e a memória que o próprio runtime usa. `SIZE` é um número de bytes e pode ser seguido de `K`, `M` ou `G` (1024, 1024² e 1024³ vezes). Uma alocação que passaria do limite é informada, e o programa termina com o código de saída 1. Essa saída não pode ser capturada com `catch`. Sem a opção, não há limite.
- `--no-cons` cria um executável que não usa células cons. Ele não tem arena de cons, então um programa que faça algo que precise de células cons é um **erro de compilação**, que mostra essa operação e a cadeia de funções desde `main` que chega a ela. Células cons são necessárias para construir uma lista `Sexpr` (incluindo uma lista citada e uma construída com quasiquote) ou um argumento `&rest`, e para `read`, `eval` e `macroexpand`. Operações com números, strings, vetores, tabelas hash, structs, canais e afins, e `print`/`println`/`format`, não usam nenhuma. Não pode ser passado junto com `--heap-cells` e é escrito depois de `-c`.

`--max-memory` é escrito depois de `-c`. `--heap-cells` pode ser escrito antes ou depois de `-c`, e então é a capacidade da arena de cons do executável. A arena de cons conta para o limite de memória, então com um limite em que a arena não cabe o programa para antes de começar.

### 3.6 Compilar para WebAssembly

Com `--target wasm32-wasip1`, `typl -c` cria um módulo WebAssembly (um comando WASI preview 1) em vez de um executável. Sem `-o`, o nome é o da fonte com `.typl` trocado por `.wasm`.

```sh
$ typl -c hello.typl --target wasm32-wasip1      # cria hello.wasm
$ wasmtime hello.wasm a b                        # executar com um ambiente compatível com WASI
args: #(hello.wasm a b)
fib(25) = 75025
```

- Um `typl` instalado com `install.sh` pode ser usado como está. Um instalado com `cargo install` exige preparação (que também vale para a seção 3.7). Primeiro execute `rustup target add wasm32-wasip1 wasm32-unknown-unknown` com o rustup. Depois instale-o com `--features wasm` (`cargo install --locked --path . --features wasm`).
  Um `typl` instalado sem isso não tem o runtime de WebAssembly, e `--target` é um erro. Para usar uma build de depuração do `typl` no repositório, execute `scripts/build-wasm-runtime.sh` no repositório.
- A ligação usa o linker da toolchain do rustup (`rust-lld`), então o rustup também é necessário ao compilar.
- O módulo se comporta como o executável: saída padrão, erro padrão e código de saída, `(command-line-args)`,
  leitura e escrita de arquivos, `throw`/`catch`/`panic`, tarefas, canais e `sleep`, e `eval` funcionam.
  As tarefas rodam uma após a outra em uma única thread. Só é possível ler e escrever arquivos dentro dos
  diretórios que o ambiente mostra ao módulo (no wasmtime, indicados com `--dir`).
- Um programa que usa algo que o WebAssembly não tem é um **erro de compilação**. O erro mostra o que foi usado e
  a cadeia de funções a partir de `main` que chega até lá. Não podem ser usados: sockets (TCP, UDP, domínio Unix,
  TLS, resolução de nomes), threads do sistema (`Thread`), funções C (`defffi`), `get-internal-run-time`, `machine-instance`, `software-version`, `file-author`
  e `ed`.
- O WASI não tem como saber o fuso horário, então a hora local é desconhecida. `decode-universal-time` e `encode-universal-time` sem `zone`, e `get-decoded-time`, entram em panic em tempo de execução. Funcionam quando `zone` é passado.
- `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` e `--release` funcionam como para um executável.

### 3.7 Executar em um navegador ou no Node.js

Com `--target wasm32-unknown-unknown`, `typl -c` cria um módulo WebAssembly para ser carregado e executado a partir de JavaScript. Ao lado do módulo (`hello.wasm`), na mesma pasta, é criado o JavaScript que o carrega e executa (`hello.mjs`). A preparação é a mesma da seção 3.6.

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # cria hello.wasm e hello.mjs
$ node hello.mjs                                       # executar com o Node.js
args: #()
fib(25) = 75025
```

A partir de outro JavaScript, chame `run` de `hello.mjs`. `write` recebe a saída como texto (`fd` é 1 para a saída padrão e 2 para o erro padrão). O valor de `run` é o código de saída.

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* mostrar na página, por exemplo */ } });
```

- Sem `write`, o Node.js escreve na saída padrão e no erro padrão, e um navegador no console, uma linha por vez.
- Em um navegador, sirva `hello.wasm` por HTTP do mesmo lugar que `hello.mjs` (não é possível carregá-lo de `file://`).
- `sleep` espera com `Atomics.wait`. Um navegador só permite isso dentro de um Worker de uma página com isolamento de origem cruzada, então execute em um Worker os programas que usam `sleep`.
- A hora local (`decode-universal-time` sem `zone` etc.) é calculada com o fuso horário que o `Date` do JavaScript informa.
- Além do que não pode ser usado na seção 3.6, operações com arquivos e diretórios, variáveis de ambiente, `(command-line-args)`, o diretório pessoal e `dribble` são um **erro de compilação**. A entrada padrão é lida como vazia.

### 3.8 Compilar para um microcontrolador (Cortex-M)

Com `--target thumbv7em-none-eabihf`, `typl -c` cria um executável (ELF) para uma placa Cortex-M4F ou M7 sem sistema operacional. Sem `-o`, ele recebe o nome do fonte com `.typl` trocado por `.elf`. Com `--target thumbv8m.main-none-eabihf`, é para uma placa Cortex-M33.

```sh
$ typl -c hello.typl --target thumbv7em-none-eabihf          # cria hello.elf
$ qemu-system-arm -M mps2-an386 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # execute no Cortex-M4 do QEMU
$ typl -c hello.typl --target thumbv8m.main-none-eabihf
$ qemu-system-arm -M mps2-an505 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # executar no Cortex-M33 do QEMU
```

- Um `typl` instalado com `install.sh` pode ser usado como está. Para instalar com `cargo install`, rode `rustup target add thumbv7em-none-eabihf thumbv8m.main-none-eabihf` e então instale com `--features embedded` (`--features wasm,embedded` para os dois). Com um build de depuração do `typl` no repositório, rode `scripts/build-embedded-runtime.sh`. A ligação usa o linker da toolchain do rustup.
- Informe a memória da placa com `--linker-script FILE`. Ele é escrito como o `memory.x` do `cortex-m-rt`: endereços e tamanhos de FLASH e RAM. Uma linha como `_typelisp_stack_size = 32K;` define quanto da RAM a pilha ocupa (64KiB se não for dada); o resto da RAM é o heap. Sem a opção, o layout é o do `mps2-an386` (`mps2-an505` para um Cortex-M33) do QEMU.

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x08000000, LENGTH = 512K
    RAM : ORIGIN = 0x20000000, LENGTH = 128K
  }
  _typelisp_stack_size = 16K;
  ```

- `--board-functions` escolhe quem fornece saída, entrada, término e relógio (as funções da placa).
  - `semihosting` (padrão): via semihosting, para rodar no QEMU (`-semihosting-config enable=on`) ou com um depurador.
  - `c`: a placa as fornece como funções C. Passe com `--link FILE` um objeto ou biblioteca estática que defina as funções abaixo (`--link` pode ser repetido; uma biblioteca do fabricante chamada por `defffi` é passada do mesmo jeito).

  ```c
  void     typelisp_host_write(int32_t fd, const uint8_t *p, size_t len); // fd 1 é a saída padrão, 2 a saída de erro
  size_t   typelisp_host_read(int32_t fd, uint8_t *p, size_t len);        // fd 0; bytes lidos, 0 no fim
  void     typelisp_host_exit(int32_t code);                              // não retorna
  uint64_t typelisp_host_now_us(void);                                    // microssegundos de um relógio que nunca volta
  int64_t  typelisp_host_unix_us(void);                                   // microssegundos desde 1970-01-01
  void     typelisp_host_sleep_us(uint64_t us);                           // espera us microssegundos
  ```

- Os registradores de uma placa (periféricos mapeados em memória) são lidos com `volatile-read-u8`, `volatile-read-u16` e `volatile-read-u32`, e escritos com `volatile-write-u8`, `volatile-write-u16` e `volatile-write-u32`. O endereço é um `int`, e o valor lido tem o tipo da largura (`u8` etc.). Cada um lê ou escreve exatamente uma vez, onde está escrito; o compilador não o omite nem o move. Todos só podem ser usados dentro de `unsafe`. Um endereço negativo, ou que não seja múltiplo da largura, é um erro em tempo de execução antes de qualquer leitura ou escrita.

  ```lisp
  ;; UART 0 do mps2-an386 do QEMU: dados 0x40004000, estado 0x40004004, controle 0x40004008
  (defun put-byte ((c u32)) ()
    (unsafe
      (loop (when (= (logand (volatile-read-u32 #x40004004) 1) 0) (break)))   ; esperar enquanto envia
      (volatile-write-u32 #x40004000 c)))

  (defun main () ()
    (unsafe (volatile-write-u32 #x40004008 1))                               ; habilitar o envio
    (put-byte 104) (put-byte 105) (put-byte 10))                             ; "hi" e uma quebra de linha
  ```

- Escreva a função chamada quando chega uma interrupção como um `defun` com o atributo `#[interrupt(N)]` (a interrupção de periférico `N`) ou um como `#[exception(SysTick)]`. Habilite uma interrupção com `(enable-interrupt N)`. Um tratador roda na hora; só quando chega no meio do trabalho do runtime ele roda depois que esse trabalho termina. Um tratador não pode esperar (`sleep` é um erro de compilação). Os detalhes estão na seção 3.17 da [referência de sintaxe](../reference/syntax.md).

  ```lisp
  (use std::board::enable-interrupt)

  #[interrupt(5)]
  (defun on-five () ()
    (println "interrupt 5"))

  (defun main () ()
    (enable-interrupt 5)
    (unsafe (volatile-write-u32 #xE000E200 (as u32 32))))   ; gerar a interrupção 5 por conta própria (o registrador de pendentes do NVIC)
  ```

- O runtime inicia o programa (a tabela de vetores e o preparo da RAM) e aloca a memória. Um HardFault ou uma interrupção não tratada encerra o programa com o código de saída de um panic (1).
- `throw`/`catch`/`panic`, bignums, frações e números de ponto flutuante, e tarefas, canais e `sleep` se comportam como no executável. As tarefas rodam em turnos no único núcleo. Funções C (`defffi`) podem ser usadas.
- Um programa que usa o que a placa não tem é um **erro de compilação**: além do que não pode ser usado na seção 3.7, `eval` e `macroexpand`. O horário local é desconhecido (como na seção 3.6).
- A arena de cons começa com 1024 células e cresce quando precisa. `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` e `--release` funcionam como estão.

### 3.9 Compilar para Arduino Uno (AVR)

Com `--target avr-atmega328p`, é criado um executável (ELF) para o microcontrolador do Arduino Uno (ATmega328P). O programa a seguir faz piscar o LED da placa.

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)                         ; PB5

(defun wait-a-bit () ()
  (let ((i (the u32 0)))
    (while (< i (the u32 400000))
      (unsafe (volatile-read-u8 PINB))
      (setf i (+ i 1)))
    ()))

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED))           ; configura como saída
  (while true
    (unsafe (volatile-write-u8 PINB LED))         ; inverte
    (wait-a-bit)))
```

```sh
$ typl -c blink.typl --target avr-atmega328p -O2   # cria blink.elf
$ qemu-system-avr -M uno -display none -serial stdio -bios blink.elf   # executa no QEMU
```

- Não há runtime, portanto um programa que o usa é um **erro de compilação**. O erro diz o que é necessário e por quais funções se chega a isso. Podem ser usados: cálculos com inteiros de largura fixa (de `i8` a `u32`), `bool` e `char`, chamadas de função, condicionais, laços, `defvar` e `defconstant` com esses valores, e `volatile-read-u8` e similares com o endereço escrito como número ou constante.
- Não podem ser usados: valores que ficam no heap, como strings, listas e estruturas; cálculos com `int`; recursão, `catch` e chamadas de valores de função; chamadas de funções C; tratadores de interrupção; divisão por uma variável (a divisão por um número pode ser usada); acessos `volatile` cujo endereço é calculado.
- Um laço que não faz nada é removido em `-O1` ou acima. Em um laço de espera, leia um registrador, por exemplo.
- Quando `main` termina, o microcontrolador para.
- A disposição da memória é alterada com `--linker-script FILE`. Sem ele, são 32 KiB de flash menos os últimos 512 bytes do bootloader, e 2 KiB de RAM. Os endereços de RAM são escritos somando `0x800000`, como `0x800100`.
- Cálculos com `f32` e `f64` são um erro de ligação, porque não há funções auxiliares para AVR.
- A ligação usa o linker da toolchain do rustup, como na seção 3.8.

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
