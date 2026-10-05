<!-- translated-from: docs/ja/guide/ffi.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# FFI de C (defffi)

Este guia explica como chamar funções C a partir do typelisp. A lista de tipos que podem ser declarados e as
restrições estão em [Referência de sintaxe 3.3](../reference/syntax.md#33-defffi--declarar-funções-c-ffi).

## 1. Declarar e chamar uma função

`defffi` declara o nome e os tipos de uma função C.

```lisp
(defffi (c-getpid "getpid") () i32)            ; o nome no typelisp e o nome do símbolo em C
(defffi (c-strlen "strlen") (string) c-ulong)
(defffi (c-cos "cos") (f64) f64 :library "m")  ; procura na libm
```

As chamadas são envolvidas em `(unsafe ...)`.

```lisp
(unsafe (c-getpid))        ; => 12345
(unsafe (c-cos 0.0))       ; => 1.0
```

`unsafe` é necessário porque o compilador não consegue verificar se os tipos declarados batem com os tipos
reais do lado C. Escrever `unsafe` significa que você, quem escreve, assume a responsabilidade por essa
verificação. Esquecê-lo dá um erro que explica isso.

## 2. Escrever um invólucro seguro

O uso pretendido é confinar o `unsafe` em um só lugar e apresentar uma função comum para fora.

```lisp
(defun pid () i32 (unsafe (c-getpid)))
(defun str-len ((s string)) int (as int (unsafe (c-strlen s))))

(pid)              ; quem chama não precisa de unsafe
(str-len "hello")  ; => 5
```

## 3. Como os tipos se correspondem

| typelisp | C |
|---|---|
| `i8` `i16` `i32` / `u8` `u16` `u32` | Inteiros da mesma largura |
| `f32` / `f64` | `float` / `double` |
| `bool` | `bool` (`_Bool`) |
| `()` | `void` |
| `string` | `const char *` |
| `c-long` / `c-ulong` | `long` / `unsigned long` (também `size_t`, `int64_t` etc.) |
| `ptr` | Qualquer ponteiro (`void *`, `FILE *` etc.) |
| `(ptr T)` | Um ponteiro para `T` ([seção 7](#7-structs-c)) |

### Strings

- Uma `string` que você passa é copiada para uma string C terminada em NUL, que é liberada depois que a
  chamada retorna. Um NUL no meio da string é um erro.
- O resultado de uma função que devolve `string` também é copiado. A memória do lado C não é liberada. Para
  funções que devolvem uma string que quem chama deve liberar (como `strdup`), receba o resultado como `ptr` e
  faça o `free` você mesmo.
- Se uma função declarada para devolver `string` devolver NULL, é um erro. Receba como `ptr` o resultado de
  funções que podem devolver NULL (como `getenv`).

### `c-long` / `c-ulong` / `ptr`

Esses tipos existem só para passar valores pela fronteira com C, e **não suportam aritmética**. Para usar um
deles como inteiro do typelisp, converta-o com `as`.

```lisp
(as int (unsafe (c-strlen s)))      ; int não perde nada do valor de 64 bits
(try-as i32 (unsafe (c-strlen s)))  ; none se não couber em um i32
(unsafe (c-malloc 16))              ; literais inteiros podem ser passados como estão
```

Um `ptr` é um valor para ser devolvido a funções C. Não há como ler, do lado do typelisp, aquilo para que ele
aponta.

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())

(unsafe (let ((p (c-malloc 16)))
          (c-free p)
          ()))
```

Esses tipos só podem aparecer como argumentos de função, valores de retorno e variáveis locais. Não podem ser
campos de estrutura, variáveis globais nem argumentos de tipo de `Vector` e afins.

## 4. Indicar uma biblioteca

Sem `:library`, o símbolo é procurado no que já está ligado ao processo (libc etc.). Funções de outras
bibliotecas precisam de `:library`.

```lisp
(defffi (sqlite-version "sqlite3_libversion") () string :library "sqlite3")
```

- Um nome curto como `"sqlite3"` é procurado como `libsqlite3.dylib` e depois como `libsqlite3.so`.
- Um nome que contém `/` é tratado como caminho.
- Se o símbolo declarado não for encontrado, o erro o nomeia.

## 5. Compilação AOT

Programas que usam `defffi` podem virar executáveis com
[`compile-file`](compile.md#3-construir-um-executável-com-compilação-aot) como estão. As bibliotecas indicadas
com `:library` são adicionadas automaticamente na ligação, então `compile-file` não precisa de argumentos
extras.

## 6. Callbacks

Você pode passar uma função typelisp para uma função C e fazer com que ela seja chamada de volta. Escreva um
tipo de função entre os tipos de argumento de `defffi` e, na chamada, ponha nessa posição um nome de função ou
uma expressão `lambda`.

```lisp
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (text-of "strstr") (ptr string) string)          ; strstr(p, "") devolve p
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun sort-chars ((s string)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (c-qsort buf (as c-ulong (length s)) 1
               (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
      (let ((r (text-of buf ""))) (c-free buf) r))))

(sort-chars "cadb")    ; => "abcd"
```

- Só podem ser passadas funções **sem variáveis livres**. Funções de nível superior, `lambda`s e funções
  locais de `labels` funcionam, mas referir-se a uma variável local de um escopo envolvente é um erro na
  verificação de tipos. O C só passa os argumentos declarados, então não há como entregar variáveis
  capturadas. Para guardar estado, use variáveis globais.
- Não é possível passar uma variável que contém uma função. Escreva no lugar um nome de função ou uma
  expressão `lambda`.
- Um `panic` ou `throw` dentro do callback chega a quem chamou depois que a função C retorna.
- O callback só pode ser chamado enquanto a função C que o typelisp chamou está em execução. Ele não pode ser
  usado a partir de coisas como `atexit` ou tratadores de sinal.

## 7. Structs C

Para passar algo como um array de structs a uma função C, declare uma struct com o mesmo layout que em C
usando `def-c-struct`, e aloque-a dentro de `unsafe`.

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))    ; declarada dentro de um unsafe de nível superior

(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))

(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))                    ; quatro itens, todos zerados
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8
               (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1))
         (* 10 (key-at xs 2)) (key-at xs 3)))))

(sorted-keys)    ; => 1234
```

- `(c-alloc T n)` aloca `n` valores de `T` e devolve um `(ptr T)`. `(c-ref p i)` é um ponteiro para o
  `i`-ésimo, `p::field` é um campo e `(c-deref p)` é aquilo para que aponta um ponteiro para um escalar como
  `i32`. Todos podem ser escritos com `setf`.
- `(as ptr p)` o transforma em um `ptr` sem tipo para passá-lo a funções C que recebem um `void *`.
- O tamanho de `item` (8 aqui) e a posição de cada campo são determinados pelas mesmas regras do C.

### Tempo de vida da memória alocada

A memória alocada é liberada quando o controle sai do `unsafe` mais externo daquela função. O mesmo acontece
quando se sai por `panic` ou `throw`. Por isso, um valor `(ptr T)` não pode ser levado para fora do `unsafe`.
Torná-lo o valor do `unsafe`, capturá-lo em uma closure, passá-lo a um `task` e lançá-lo com `throw` são
todos erros de tipo. Copie os valores que você quer usar fora para números ou para um `defstruct` dentro do
`unsafe`.

Ao alocar dentro de uma `lambda` ou de uma função de `labels`, escreva um `unsafe` dentro dessa função.

### Memória alocada pelo C

Um ponteiro recebido do C como `(ptr T)` (um valor de retorno de `defffi`, um argumento de callback etc.) é um
erro a menos que aponte para dentro de memória alocada com `c-alloc`. Declare com o `ptr` sem tipo as funções
que recebem memória que o C alocou com `malloc`, ou NULL.

## 8. O que não é possível fazer

- **Funções variádicas** (`printf` e afins) não podem ser declaradas. A parte variádica é passada com regras
  diferentes das dos argumentos fixos. Declare um nome separado para cada número de argumentos que você usa.
- **Passar ou devolver structs por valor** não é possível. Use funções que passam ponteiros.
- **Declarações genéricas** não são possíveis.
- **O mesmo nome de uma função embutida** não pode ser usado.
- **Elas não podem ser passadas como valores de função.** Não é possível passar uma como em `(map xs c-abs)`;
  envolva-a em uma `lambda`.

  ```lisp
  (unsafe (lambda ((n i32)) i32 (c-abs n)))
  ```
