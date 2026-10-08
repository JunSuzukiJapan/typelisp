<!-- translated-from: docs/ja/tutorial/types.md @ fc3823e182015d6a1ef25d03ecdf8ca958af01f2 -->
# Fundamentos de tipos

typelisp é uma linguagem com tipagem estática. Este capítulo explica o que o verificador de tipos faz por
você, os tipos que você mais vai usar (`Option`, `Result`, estruturas e enumerações) e os genéricos. Ele
pressupõe que você leu [Primeiros passos](intro.md).

## 1. O que significa tipagem estática

No typelisp, o tipo de toda expressão fica determinado antes de o programa ser executado. Uma expressão cujos
tipos não batem é um erro antes que qualquer coisa seja executada.

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; erro de tipo

(main)
```

Executar este arquivo para com um erro de tipo sem sequer imprimir `start`.

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

É preciso escrever tipos para os argumentos e valores de retorno das funções, as variáveis globais e os
campos das estruturas. O tipo de uma variável de `let` vem do seu valor inicial.

Os principais tipos:

| Tipo | Valores de exemplo |
|---|---|
| `int` | `42`, `-7` (inteiros de precisão arbitrária) |
| `i8` `i16` `i32` `u8` `u16` `u32` | Inteiros de largura fixa |
| `f64` `f32` | `1.5` |
| `bool` | `true`, `false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`, `:key` |
| `()` | O tipo de retorno de uma função que não devolve valor |

Não há como perguntar o tipo de um valor em tempo de execução (não existem `typep` nem `type-of` do Common
Lisp), porque todo tipo já é conhecido antes de o programa ser executado.

## 2. `Option<T>`: um valor que pode faltar

typelisp não tem `nil`. "Pode não haver valor" é expresso com o tipo `Option<T>`. Um valor de `Option<T>` é
ou `some`, que contém um valor de `T`, ou `none`, que não contém nada.

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>` não é `int`, então não pode ser usado em aritmética como está. `(+ (safe-div 10 2) 1)` é um
erro de tipo. Para usar o que há dentro, separe `some` de `none` com `match`.

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- No ramo `(some q)`, o conteúdo fica vinculado à variável `q`.
- `match` verifica se seus ramos **cobrem todos os casos**. Esquecer o ramo `(none)` é um erro de tipo.

### Por que não há nil

Em muitas linguagens, `nil` (`null`) pode ocupar o lugar de um valor de qualquer tipo. Como resultado,
esquecer de tratar o caso "sem valor" passa despercebido até o programa ser executado. No typelisp, um lugar
onde um valor pode faltar tem o tipo `Option<T>`, e o código não passa pelo verificador de tipos a menos que
`match` trate o caso `none`. Um caso esquecido é descoberto antes de o programa ser executado.

As condições seguem a mesma ideia: somente um `bool` pode ser a condição de `if`. Não existe uma regra como
a do Common Lisp de que "tudo o que não é `nil` é verdadeiro".

### Operações comuns

| Forma | Significado |
|---|---|
| `(unwrap-or opt default)` | O conteúdo se for `some`; o valor padrão se for `none` |
| `(unwrap opt)` | Tira o conteúdo. Interrompe o programa se for `none` |
| `(is-some opt)` / `(is-none opt)` | Testa qual dos dois é |

Muitas funções da biblioteca padrão devolvem `Option`. Por exemplo, `position` devolve a posição dentro de
`some` se encontrar o elemento, e `none` se não encontrar.

```lisp
(defvar (fruits Vector<string>) (Vector::new))
(push fruits "apple")
(push fruits "banana")

(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`: uma operação que pode falhar

Uma operação que pode falhar devolve `Result<T,E>`: `ok`, que contém um valor de `T` em caso de sucesso, ou
`err`, que contém um erro `E` em caso de falha.

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

Suas próprias funções também podem devolver `Result`.

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

Use `Option` quando a falta de um valor não precisa de explicação, e `Result` quando quiser dizer por que
algo falhou. [Tratamento de erros](errors.md) aborda em detalhes o tratamento de erros.

## 4. `defstruct`: estruturas

Um tipo com campos nomeados é definido com `defstruct`.

```lisp
(defstruct point
  (x int)
  (y int))
```

A definição lhe dá o seguinte:

```lisp
(let ((p (point::new 3 4)))     ; criar um (argumentos na ordem dos campos)
  (println "~a" p::x)           ; ler um campo; (x p) também funciona
  (setf p::x 10)                ; mudá-lo
  (println "~a" p))             ; #<point x: 10 y: 4>
```

Para dar a uma estrutura funções próprias, use `defmethod`. O tipo do primeiro argumento (`self`) decide a
qual tipo o método pertence.

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

Escrever só o nome do tipo em vez de um argumento `self` cria uma função chamada como `point::origin`.

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`: uma de várias formas

Um valor que é uma de várias formas, como "um círculo, um retângulo ou um ponto", é definido com `defenum`.
Cada forma se chama **variante**. Cada variante pode conter um número e tipo de valores diferentes.

```lisp
(defenum shape
  (circle int)        ; raio
  (rect int int)      ; largura e altura
  (dot))              ; não contém nenhum valor
```

Os valores são criados com o nome do tipo na frente, como em `shape::circle`. No `match`, eles são desmontados
pelo nome da variante.

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

Aqui também `match` verifica se todos os casos são cobertos. Se mais tarde você adicionar uma variante a
`shape`, todo `match` que não a trate vira um erro de tipo, então nenhum lugar que precise de correção passa
despercebido.

Depois de `(use shape)`, você pode escrever `(rect 5 6)` sem o nome do tipo.

`Option` e `Result` são enumerações construídas com esse mesmo mecanismo.

## 6. Genéricos

Uma função que serve para qualquer tipo é definida com um **parâmetro de tipo** `<T>` depois do nome.

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

Ao chamá-la, você não informa o tipo. `T` é deduzido dos argumentos.

```lisp
(defvar (ints Vector<int>) (Vector::new))
(defvar (names Vector<string>) (Vector::new))
(push names "Ann")

(first-or ints 7)          ; T é int
(first-or names "none")    ; T é string
(first-or ints "none")     ; erro de tipo: ints é um Vector<int>, então T é int
```

Estruturas e enumerações também podem ser genéricas. `Vector<T>`, `Option<T>` e `Result<T,E>` são tipos
desse tipo.

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

Dentro de uma função genérica nada se sabe sobre `T`, então não é possível comparar nem somar valores de
`T`. Para exigir algo como "qualquer tipo que possa ser comparado", use traits ([Traits](traits.md)).

## 7. Dar outro nome a um tipo

`deftype` dá outro nome a um tipo.

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters` é só outra forma de escrever `int`, não um tipo novo. Passar um `int` comum onde se espera `meters`
não é um erro. Se quiser mantê-los separados, crie uma estrutura, como em `(defstruct meters (value int))`.

## 8. O que ler em seguida

- [Traits](traits.md): dar aos tipos operações em comum
- [Tipos](../reference/types.md): os tipos embutidos e os traits que cada um implementa
