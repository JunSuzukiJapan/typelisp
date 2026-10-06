<!-- translated-from: docs/ja/guide/from-common-lisp.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Para programadores de Common Lisp

typelisp herda a sintaxe do Common Lisp (CL) e muitos dos seus nomes de função, mas é uma linguagem com
tipagem estática. Por isso, código CL nem sempre funciona como está escrito. Este guia reúne os pontos em que
quem está acostumado ao CL costuma tropeçar, junto com como reescrever o código.

## 1. Não existem `nil` nem `t`

Os valores booleanos são `true` e `false`. `nil` e `t` não estão definidos.

```lisp
(if (> x 0) "pos" "non-pos")
(defvar (debug bool) false)
```

- **Só um `bool` pode ser condição.** Escrever `0` ou uma lista vazia como condição é um erro de tipo. Não
  existe a regra de que "tudo o que não é nil é verdadeiro".
- **O ramo else do `if` não pode ser omitido.** `(if c x)` é um erro. Quando não for preciso ramo else, use
  `when` / `unless`.
- **"Sem valor" é expresso com `Option<T>`.** Uma função que no CL devolvia nil para indicar "não encontrado"
  aqui devolve `(some x)` ou `none`.

  ```lisp
  (match (position 3 (iter v))
    ((some i) (println "found at ~a" i))
    ((none) (println "not found")))
  ```

- A lista vazia `()` é, conforme o contexto, o valor do tipo Unit (o valor de retorno de uma função que não
  devolve nada) ou a lista vazia dos dados de expressões S. É um valor diferente de `false`.

## 2. Escrever tipos

Os argumentos e os valores de retorno das funções precisam ter tipos.

```lisp
(defun area ((w i32) (h i32)) i32
  (* w h))

(defun first-or<T> ((v Vector<T>) (default T)) T      ; uma função genérica
  (unwrap-or (first (iter v)) default))
```

- Não é possível escrever uma definição sem tipos, como `(defun f (x) x)`.
- Variáveis globais como `defvar` também precisam de tipo: `(defvar (count int) 0)`.
- `the` não é uma verificação em tempo de execução, mas uma anotação para o verificador de tipos.
- **Não há como inspecionar tipos em tempo de execução.** Não existem `typep` nem `type-of`, porque o tipo de
  todo valor é fixado em tempo de compilação. Para aceitar um de vários tipos, crie um tipo soma com
  `defenum` ou use um trait.
- `deftype` define um apelido de tipo. Não é possível criar um tipo que descreva uma faixa de valores, como
  `(deftype small () '(integer 0 9))`.

O tipo inteiro padrão `int` tem precisão arbitrária; como o integer do CL, não há limite superior para seu
tamanho. Também existem os tipos de largura fixa `i8` a `i32` e `u8` a `u32`. Não há tipo inteiro de largura
fixa de 64 bits.

## 3. Funções como valores

typelisp não separa os namespaces de funções e de variáveis. O nome de uma função pode ser passado como valor
como está. Não há `#'` nem `funcall`.

```lisp
(defun twice ((x int)) int (* 2 x))
(defun apply-to ((f (fn (int) int)) (x int)) int
  (f x))                                  ; chama diretamente, não com funcall

(apply-to twice 5)                        ; twice, não #'twice
```

- Funções embutidas como `+` e `1+` também podem ser passadas como valores como estão, onde o tipo do
  argumento é fixo, como em `(fn (int) int)`. Ao passar uma delas para uma função genérica como `foldl` ou
  `map`, não se sabe a qual `+` de qual tipo se refere, então envolva-a em uma `lambda`.

  ```lisp
  (apply-to 1+ 5)                                          ; => 6
  (foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)
  ```

- As funções de sequência recebem **primeiro a coleção e depois a função**: `(map it f)`, `(filter it f)`,
  `(foldl it f init)`. É o contrário do `(mapcar f list)` do CL.
- `lambda` não pode usar `&optional` nem `&key` (`&rest` pode).
- **Uma função não pode ser chamada antes de ser definida.** No CL você pode chamar uma função que define
  depois, mas aqui isso dá `no such function`. Para funções mutuamente recursivas, declare primeiro uma delas
  com `defsignature`.

  ```lisp
  (defsignature odd2 (i32) bool)
  (defun even2 ((n i32)) bool (if (= n 0) true (odd2 (- n 1))))
  (defun odd2 ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
  ```

## 4. Listas e Vector

O que corresponde a uma lista do CL são os **dados de expressões S**, cujo tipo é `Option<Sexpr>` (a lista
vazia é `none`). `(list 1 2 3)` e `'(a b c)` têm esse tipo. Os dados de expressões S são algo com que as
macros e `read` trabalham; para um contêiner de dados comum, use **`Vector<T>`**.

| O que você quer | CL | typelisp |
|---|---|---|
| Cabeça e resto de uma expressão S | `(car xs)` `(cdr xs)` | `(sexpr-car xs)` `(sexpr-cdr xs)` |
| Percorrer uma lista de expressões S | `(dolist (x xs) ...)` | Igual |
| Uma sequência de elementos de um mesmo tipo | Uma lista ou um vetor | `Vector<T>` |
| Um par | `(cons a b)` | `(cons a b)` (seu tipo é `cons-cell<A,B>`) |
| Mapeamento | `(mapcar f xs)` | `(map (iter v) f)` |

`car` / `cdr` são os acessores do par `cons-cell<A,B>` criado com `cons`. Não podem ser usados em listas de
expressões S.

Criar um `Vector`:

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 1)
  (push v 2)
  (map (iter v) (lambda ((x int)) int (* x 10))))   ; => #<vector<int> 10 20>
```

As funções de sequência como `map`, `filter`, `sort` e `find` trabalham sobre valores que implementam o trait
`Iter`. Passe um `Vector` depois de transformá-lo em iterador com `(iter v)`.

## 5. Não existem valores múltiplos

Não existem `values` nem `multiple-value-bind`. Funções que no CL devolvem vários valores aqui devolvem um par
ou uma estrutura.

| CL | typelisp |
|---|---|
| `(floor 7 2)` → 3, 1 | `(floor-div 7 2)` → um `cons-cell` cujo `car` é 3 e cujo `cdr` é 1 |
| `(decode-universal-time t)` → 9 valores | Uma estrutura `decoded-time` |
| `(read-from-string s)` → valor, posição | `(read-from-string s)` devolve um `cons-cell` de valor e posição dentro de um `Result`. Só para o valor, `(read s)` |

## 6. Não existem variáveis especiais (vínculo dinâmico)

`let` sempre vincula lexicamente. Se você revincular com `let` uma variável definida com `defvar`, as funções
chamadas a partir dali continuam vendo o valor original.

```lisp
(defvar (*depth* int) 1)
(defun show () () (println "~a" *depth*))
(let ((*depth* 2)) (show))       ; 2 no CL, 1 no typelisp
```

Para mudar temporariamente uma variável de controle como `*print-base*`, use `dlet`. Ele atribui o valor e
restaura o original qualquer que seja a forma de sair do corpo.

```lisp
(dlet ((*print-base* 16))
  (format false "~a" 255))       ; => "ff"
```

`dlet` reescreve a própria variável global, então não é um vínculo por thread.

## 7. O sistema de condições não é adotado

Não existem `define-condition`, `handler-case`, `handler-bind`, `restart-case`, `error` nem `signal`. Eles
combinam mal com a tipagem estática. Em vez disso, usam-se estas duas coisas para propósitos diferentes:

- **Falhas recuperáveis devolvem `Result<T,E>`.** Quem chama separa `ok` / `err` com `match`. Não há uma forma
  abreviada como o `?` do Rust.

  ```lisp
  (match (parse-int "42x")
    ((ok n) n)
    ((err e) (progn (println "bad input: ~a" (message e)) 0)))
  ```

- **Falhas irrecuperáveis (bugs) são `panic`.** `(panic "message")`, passar `none` para `unwrap`, dividir por 0
  e um índice fora do intervalo são desse tipo, e o programa para. A limpeza de `unwind-protect` é executada
  antes de parar.

Os tipos de erro são unificados pelo trait `Error`, e `(message e)` dá a mensagem. Como criar seu próprio tipo
de erro está em
[Option, Result e tipos de erro](../reference/functions/option-result.md#3-tipos-de-erro-e-o-trait-error).
`assert` e `warn` podem ser usados como no CL.

`catch` / `throw` / `unwind-protect` existem. Porém, a etiqueta de `catch` se limita a um símbolo literal não
avaliado (`'done`), e os valores lançados com uma etiqueta têm um único tipo.

## 8. Não existe CLOS

Não existem `defclass`, `defgeneric` nem combinação de métodos.

- Os tipos de dados são definidos com `defstruct` (estruturas) e `defenum` (tipos soma).
- `defmethod` define métodos cujo alvo é decidido somente pelo **tipo estático do primeiro argumento**. Não há
  despacho múltiplo.
- Para dar operações em comum a vários tipos, use traits (`deftrait` / `impl`). Para valores cujo tipo
  concreto é decidido em tempo de execução, use o tipo `:dyn Trait`
  ([Referência de sintaxe 3.9](../reference/syntax.md#39-deftrait--impl--traits)).

Em que o `defstruct` difere:

- O construtor é `NomeDoTipo::new`: `(point::new 1 2)`. Se você quiser um nome como `make-point`, a opção
  `(:constructor make-point)` o cria.
- Além de `(x p)`, um acessor pode ser escrito `p::x`. Mude-o com `(setf p::x 5)`.
- Nenhum predicado (`point-p`) é criado. Não há `:conc-name`, `:type` nem `:named`.
- `:include` só herda os slots; o tipo não vira subtipo do pai.

## 9. Módulos em vez de pacotes

Não há pacotes. Os namespaces são módulos, e um arquivo é um módulo por si só. Em vez de `pkg:symbol`,
escreve-se `module::name`, e os nomes são trazidos com `use` ([Módulos e organização de arquivos](modules.md)).

As palavras-chave `:foo` existem e são símbolos que avaliam para si mesmos. Como não há pacotes, os dois-pontos
fazem parte do nome: `(symbol->string :foo)` devolve `":foo"`.

## 10. Diferenças de leitura e sintaxe

- Maiúsculas e minúsculas não são distinguidas (os símbolos viram minúsculas ao serem lidos). Igual ao CL.
- Não há `#'` (seção 3). Literais de números complexos `#c(...)` não podem ser lidos; crie números complexos
  com `(complex 1.0 2.0)`.
- As cláusulas do `loop` estendido são escritas com palavras-chave:
  `(loop :for i :from 1 :to 3 :collect i)`. Um `loop` que não começa com uma palavra-chave é um simples laço
  infinito, do qual se sai com `(break)` ou `(return valor)`. `return` sai do laço mais interno (para sair de
  uma função, use `return-from`).
- O destino de `format` é `false` (devolver uma string), `true` (saída padrão) ou um stream. As diretivas de
  formato são as mesmas do CL.
- Ler de uma string é `(read "...")`, e ler de um stream é `(read-sexpr s)`. Ambos devolvem um `Result`.
- `eval` verifica os tipos da expressão dada antes de avaliá-la e devolve um `Result`. Referências antecipadas
  não são possíveis, assim como no código-fonte.
- Não existe `eval-when`.
- Os nomes de função não usam os sufixos `?` nem `!`. Os predicados são nomeados com `-p` / `p` como no CL
  (`zerop`, `sexpr-null`) ou com `is-` na frente (`is-some`).

## 11. Principais funções com nomes diferentes

| CL | typelisp |
|---|---|
| `string-upcase` / `string-downcase` | `upcase` / `downcase` |
| `read` (de um stream) | `read-sexpr` |
| `pathname` | `to-pathname` |
| Versões de dois argumentos de `floor` e afins | `floor-div` `ceiling-div` `round-div` `truncate-div` |
| `mapcar` | `map` (ordem dos argumentos invertida; seção 4) |
| `length` (de um vetor) | `len` |
| `hash-table-count` | `count` / `size` |

A lista de funções está em [Funções embutidas](../reference/functions/README.md).

## 12. Outras coisas que não existem

- `progv`, `symbol-function`, `symbol-value`
- `*readtable*` e `copy-readtable`, `readtable-case` (as macros de leitura em si podem ser definidas com
  `set-macro-character`)
- Nomes de caminho lógicos e nomes de caminho com curingas
- `input-stream-p` / `output-stream-p` (a direção de um stream é decidida pelo seu tipo)
