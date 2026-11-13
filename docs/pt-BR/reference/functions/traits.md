<!-- translated-from: docs/ja/reference/functions/traits.md @ cedb434055399f2f83e17dcdaa55007fe4fc9dad -->
# Traits padrão

Os traits para iteração, comparação, aritmética e adição e remoção de elementos. Os outros traits padrão estão nos seus próprios capítulos:
`Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Tipos de erro](option-result.md#3-tipos-de-erro-e-o-trait-error)), `print-object`
([Impressão](printing.md#5-print-object-representação-impressa-por-tipo)), e os traits de streams e `Pathish`
([Streams e arquivos](streams-files.md)). Quais tipos implementam quais está em [Tipos](../types.md). Como
definir traits está na [Referência de sintaxe](../syntax.md#39-deftrait--impl--traits).

## 1. O trait `Iter` e a iteração

```lisp
(deftrait Iter ()
  (:type Item)
  (defmethod next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implementam `Iter` por meio de `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` respectivamente (obtenha o iterador com `(iter coleção)`). `Chan<T>` é ele
mesmo um `Iter` (`recv` faz o papel de `next`; [Canais](concurrency.md#2-chant--canais)). As listas `Sexpr` não
implementam `Iter` (seus tipos de elemento não são uniformes). Se você implementar `Iter` para seu próprio tipo,
ele pode ser percorrido com `doiter` como está e passado às
[funções de sequência](sequences.md#4-funções-de-sequência-sobre-iter).

## 2. `Equal` / `Ord` (comparação)

`Equal` é o trait que `equal` usa e corresponde ao `PartialEq` de Rust. `Ord` corresponde ao `PartialOrd`
de Rust. Eles são usados nos limites `:where` de funções genéricas para exigir que o tipo dos elementos
possa ser comparado.

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; obrigatório
(deftrait Ord (Equal)                                     ; herda de Equal
  (defmethod less (self (other Self)) bool)                  ; obrigatório
  (defmethod less-equal (self (other Self)) bool (not (less other self)))
  (defmethod greater (self (other Self)) bool (less other self))
  (defmethod greater-equal (self (other Self)) bool (not (less self other))))
```

Para implementar `Equal` escreve-se `equal`, e para `Ord` apenas `less`. As implementações padrão
completam o resto de `Ord`. `Ord` herda de `Equal`, então é preciso `impl Equal X` antes de `impl Ord X`.

`(equal a b)` chama a implementação de `Equal` do tipo de `a`. Escrevê-lo para um tipo que não implementa
`Equal` é um erro de tipo. Para comparar por identidade use `eq`/`eql`, e por estrutura, `equalp`. Esses
três podem ser usados com um valor de qualquer tipo.

Cada método de trait pode ser chamado diretamente como função (dentro de um limite `where (Equal A)`/
`(Ord A)`, ou sobre um tipo concreto que o implementa):

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` where `Equal A` | Se são iguais (o `==` de Rust) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Equal` está implementado para: todos os tipos numéricos (`i8` a `u32` / `f32` / `f64` / `int` / `ratio`),
`bool` `char` `string` `symbol` `complex`, `Sexpr`, `cons-cell<A,B>` e as tuplas (elemento a elemento,
quando os elementos são `Equal`), e `Option<T>` e `Result<T,E>` (quando são a mesma variante e o conteúdo
é `equal`). Nos tipos embutidos, o método embutido `equal` do próprio tipo é, tal como está, a
implementação de `Equal`. Números comparam com `=`, e strings pelo conteúdo, diferenciando maiúsculas e
minúsculas. `Ord` está implementado para: todos os tipos numéricos, `char` `string` e `cons-cell<A,B>`
(lexicograficamente, quando os elementos são `Ord`).

O `equal` de uma lista (`Sexpr`) é o `equal` de CL: conses comparam por estrutura, strings pelo conteúdo e
qualquer outro átomo por `eql`. Essa comparação percorre os valores em tempo de execução e não chama
nenhum método. Por isso um struct dentro de uma lista é comparado por identidade, como em CL, qualquer que
seja o `impl Equal` do seu tipo.

Os nomes dos métodos de `Ord` não coincidem com os operadores embutidos (`< <= > >=`) nem com `lt` porque
o que é embutido não pode ser redefinido e cada implementação delega a eles. Os operadores de comparação
escalares são métodos embutidos por tipo de receptor ([Números](numbers.md),
[Strings e caracteres](collections.md)). Dentro de um limite, o operador escrito é lido como o método do
trait (capítulo 3). `=` vira `equal`, e `/=` vira `(not (equal a b))`.

## 3. Traits aritméticos (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Uma camada para que o código genérico exija "um tipo que possa ser somado". **A aritmética sobre tipos
concretos usa os operadores embutidos** ([Números](numbers.md)) e não passa por esta camada.

```lisp
(deftrait Add () (defmethod add (self (other Self)) Self))
(deftrait Sub () (defmethod sub (self (other Self)) Self))
(deftrait Mul () (defmethod mul (self (other Self)) Self))
(deftrait Div () (:type Output) (defmethod div (self (other Self)) Output))
(deftrait Rem () (defmethod remainder (self (other Self)) Self))
(deftrait Bits ()
  (defmethod bit-and (self (other Self)) Self)
  (defmethod bit-or  (self (other Self)) Self)
  (defmethod bit-xor (self (other Self)) Self)
  (defmethod bit-not (self) Self)
  (defmethod shift (self (count int)) Self))                 ; a distância é sempre int (como em ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; sem métodos; uma combinação de cinco
```

**Dentro de uma restrição, você pode escrever operadores.** Quando o receptor é uma variável de tipo vinculada
por `:where`, os operadores são lidos como métodos de trait (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`,
`rem`→`remainder`, `logand`→`bit-and`, `=`→`equal`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

O método do trait não se chama `+` porque `+` é o nome de um método embutido e `impl` se recusa a redefini-lo
(`cannot redefine built-in method`). Não há `Neg`: `(- x)` se expande para `(- (- x x) x)`, então `Sub` basta.

Implementados para: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` em todos os tipos numéricos (exceto `complex`), e
`Bits` em todos os tipos inteiros e em `int`.


**`Div` nomeia seu tipo de resultado com o tipo associado `Output`.** Como no CL, `/` sobre dois
inteiros é um `rational`, então o `Output` de `int` é `rational`, e o de qualquer outro tipo numérico é
o próprio tipo. Como `Output` difere entre os tipos, `Number` não inclui `Div` (uma lista de
supertraits precisa fixar cada tipo associado). Uma função genérica que divide recebe o tipo do
resultado com `(:where (Div T (Output R)))`, ou exige uma divisão fechada em `T` com
`(:where (Div T (Output T)))` (então não se pode passar um `int`).

## 4. O trait `Push` (adicionar elementos)

```lisp
(deftrait Push ()
  (:type Item)
  (defmethod push (self (x Item)) ()))
```

Uma coleção à qual se podem adicionar elementos um de cada vez. Uma função genérica pede “um tipo ao
qual se pode adicionar `T`” com `(:where (Push C (Item T)))` e adiciona a qualquer uma dessas
coleções com o mesmo `(push c x)`.

| Tipo | O que `push` faz |
|---|---|
| `Vector<T>` | Adiciona no final |
| `Deque<T>` | Adiciona no final (`push-back`) |
| `Array<T>` | Adiciona no ponteiro de preenchimento (`push-extend`). Panic sem ponteiro de preenchimento |
| `HashSet<T>` | Coloca no conjunto (`insert`) |

```lisp
(use std::collections::Deque)
(defun push-all<C,T> ((c C) (xs Vector<T>)) () (:where (Push C (Item T)))
  (doiter (x (iter xs)) (push c x)))

(let ((v (the Vector<int> (Vector::new)))
      (d (the Deque<int> (Deque::new))))
  (push-all v (the Vector<int> #(1 2)))
  (push-all d (the Vector<int> #(3 4)))
  (println "~a ~a" v d))                                  ; #(1 2) #<deque 3 4>
```

Um tipo seu que implemente `Push` pode ser passado às mesmas funções genéricas. Também pode ser um
objeto trait, como em `:dyn Push<int>`.

## 5. O trait `Pop` (retirar elementos)

```lisp
(deftrait Pop ()
  (:type Item)
  (defmethod pop (self) Option<Item>))
```

Uma coleção da qual se pode retirar o último elemento adicionado com `push`. `none` quando está
vazia. Uma função genérica a pede com `(:where (Pop C (Item T)))`.

| Tipo | O que `pop` faz |
|---|---|
| `Vector<T>` | Retira o último elemento |
| `Deque<T>` | Retira o último elemento (`pop-back`) |
| `Array<T>` | Retira o elemento logo antes do ponteiro de preenchimento. Panic sem ponteiro de preenchimento |

Um `HashSet<T>` não tem “último elemento”, então é `Push` mas não `Pop`.

```lisp
(use std::collections::Deque)
(defun pop-all<C,T> ((c C)) Vector<T> (:where (Pop C (Item T)))
  (let ((out (the Vector<T> (Vector::new))) (more true))
    (while more
      (match (pop c)
        ((some x) (push out x))
        ((none) (progn (setf more false) ()))))
    out))

(let ((v (the Vector<int> #(1 2 3)))
      (d (the Deque<int> (Deque::new))))
  (push d 1) (push d 2)
  (println "~a ~a" (pop-all v) (pop-all d)))              ; #(3 2 1) #(2 1)
```

Como com `Push`, um tipo seu pode implementá-lo, e ele pode ser um objeto trait, como em
`:dyn Pop<int>`.
