<!-- translated-from: docs/ja/reference/functions/traits.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Traits padrão

Os traits para iteração, comparação e aritmética. Os outros traits padrão estão nos seus próprios capítulos:
`Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Tipos de erro](option-result.md#3-tipos-de-erro-e-o-trait-error)), `print-object`
([Impressão](printing.md#5-print-object-representação-impressa-por-tipo)), e os traits de streams e `Pathish`
([Streams e arquivos](streams-files.md)). Quais tipos implementam quais está em [Tipos](../types.md). Como
definir traits está na [Referência de sintaxe](../syntax.md#39-deftrait--impl--traits).

## 1. O trait `Iter` e a iteração

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implementam `Iter` por meio de `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` respectivamente (obtenha o iterador com `(iter coleção)`). `Chan<T>` é ele
mesmo um `Iter` (`recv` faz o papel de `next`; [Canais](concurrency.md#2-chant--canais)). As listas `Sexpr` não
implementam `Iter` (seus tipos de elemento não são uniformes). Se você implementar `Iter` para seu próprio tipo,
ele pode ser percorrido com `doiter` como está e passado às
[funções de sequência](sequences.md#4-funções-de-sequência-sobre-iter).

## 2. `Eq` / `Ord` (comparação)

Correspondem a `PartialEq`/`PartialOrd` do Rust (com os nomes `Eq`/`Ord`). São usados nas restrições `where` das
funções genéricas para exigir que os tipos dos elementos possam ser comparados (`sort`/`member`/`assoc` etc.).

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; precisa ser implementado
  (not-equals ((self Self) (other Self)) bool             ; implementação padrão
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; herda de Eq
  (less ((self Self) (other Self)) bool)                  ; precisa ser implementado
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
```

Para implementar `Eq` você só escreve `equals`, e para `Ord` só `less`. As implementações padrão completam o
resto. `Ord` herda de `Eq`, então é preciso `impl Eq X` antes de `impl Ord X`.

Cada método de trait pode ser chamado como função como está (dentro de uma restrição `where (Eq A)`/`(Ord A)`,
ou sobre um tipo concreto que o implementa):

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | Se são iguais (o `==` do Rust) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | Se não são iguais (`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` é implementado para: todos os tipos numéricos (`i8` a `u32` / `f32` / `f64` / `int` / `ratio`), `bool`
`char` `string` `symbol` `complex`, `Sexpr` (`eq`, isto é, identidade; usado pelos padrões de valor de
`match`) e `cons-cell<A,B>` (recursivamente, quando os elementos são `Eq`). `Ord` é implementado para: todos os
tipos numéricos, `char` `string` e `cons-cell<A,B>` (lexicograficamente, quando os elementos são `Ord`).

Os nomes dos métodos não coincidem com os operadores embutidos (`= /= < <= > >=`) nem com `eq`/`lt` porque as
funções embutidas não podem ser redefinidas, e cada implementação delega a elas. Os próprios operadores de
comparação escalares são métodos embutidos de cada tipo receptor ([Números](numbers.md),
[Strings e caracteres](collections.md)). Dentro de uma restrição, escrever os operadores os lê como os métodos
de trait (capítulo 3).

## 3. Traits aritméticos (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Uma camada para que o código genérico exija "um tipo que possa ser somado". **A aritmética sobre tipos
concretos usa os operadores embutidos** ([Números](numbers.md)) e não passa por esta camada.

```lisp
(deftrait Add () (add ((self Self) (other Self)) Self))
(deftrait Sub () (sub ((self Self) (other Self)) Self))
(deftrait Mul () (mul ((self Self) (other Self)) Self))
(deftrait Div () (div ((self Self) (other Self)) Self))
(deftrait Rem () (remainder ((self Self) (other Self)) Self))
(deftrait Bits ()
  (bit-and ((self Self) (other Self)) Self)
  (bit-or  ((self Self) (other Self)) Self)
  (bit-xor ((self Self) (other Self)) Self)
  (bit-not ((self Self)) Self)
  (shift ((self Self) (count int)) Self))                 ; a distância é sempre int (como em ash)
(deftrait Number (Add Sub Mul Div Rem Ord))               ; sem métodos; uma combinação de seis
```

**Dentro de uma restrição, você pode escrever operadores.** Quando o receptor é uma variável de tipo vinculada
por `where`, os operadores são lidos como métodos de trait (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`,
`rem`→`remainder`, `logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

O método do trait não se chama `+` porque `+` é o nome de um método embutido e `impl` se recusa a redefini-lo
(`cannot redefine built-in method`). Não há `Neg`: `(- x)` se expande para `(- (- x x) x)`, então `Sub` basta.

Implementados para: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` em todos os tipos numéricos (exceto `complex`), e
`Bits` em todos os tipos inteiros e em `int`.
