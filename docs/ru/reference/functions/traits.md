<!-- translated-from: docs/ja/reference/functions/traits.md @ 9a4e7d23b09eb3f3244bfb876df1ec57576d6874 -->
# Стандартные трейты

Трейты итерации, сравнения, арифметики, добавления и извлечения элементов. Остальные стандартные трейты описаны в собственных главах: `Hash`
([HashTable](collections.md#4-hashtablekv)), `Error` ([Типы ошибок](option-result.md#3-типы-ошибок-и-трейт-error)),
`print-object` ([Печать](printing.md#5-print-object-печатное-представление-по-типу)), а также трейты потоков и
`Pathish` ([Потоки и файлы](streams-files.md)). Какие типы что реализуют, указано в [Типах](../types.md). Как
определять трейты, описано в [Справочнике по синтаксису](../syntax.md#39-deftrait--impl--трейты).

## 1. Трейт `Iter` и итерация

```lisp
(deftrait Iter ()
  (:type Item)
  (next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` реализуют `Iter` соответственно через `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (итератор получают через `(iter коллекция)`). `Chan<T>` сам является `Iter`
(`recv` играет роль `next`; [Каналы](concurrency.md#2-chant--каналы)). Списки `Sexpr` не реализуют `Iter` (типы их
элементов неоднородны). Если реализовать `Iter` для собственного типа, его можно как есть обходить через `doiter` и
передавать [функциям последовательностей](sequences.md#4-функции-последовательностей-над-iter).

## 2. `Eq` / `Ord` (сравнение)

Соответствуют `PartialEq`/`PartialOrd` из Rust (названы `Eq`/`Ord`). Используются в ограничениях `:where` обобщённых
функций, чтобы потребовать сравнимости типов элементов (`sort`/`member`/`assoc` и т. д.).

```lisp
(deftrait Eq ()
  (equals (self (other Self)) bool)                ; нужно реализовать
  (not-equals (self (other Self)) bool             ; реализация по умолчанию
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; наследуется от Eq
  (less (self (other Self)) bool)                  ; нужно реализовать
  (less-equal (self (other Self)) bool (not (less other self)))
  (greater (self (other Self)) bool (less other self))
  (greater-equal (self (other Self)) bool (not (less self other))))
```

Для реализации `Eq` пишут только `equals`, а для `Ord` — только `less`. Остальное дополняют реализации по умолчанию.
`Ord` наследуется от `Eq`, поэтому `impl Eq X` нужен до `impl Ord X`.

Каждый метод трейта можно вызывать как функцию как есть (внутри ограничения `where (Eq A)`/`(Ord A)` или на
конкретном типе, который его реализует):

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | Равны ли они (`==` в Rust) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | Не равны ли они (`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` реализован для: всех числовых типов (`i8`–`u32` / `f32` / `f64` / `int` / `ratio`), `bool` `char` `string`
`symbol` `complex`, `Sexpr` (`eq`, то есть идентичность; используется образцами-значениями `match`) и
`cons-cell<A,B>` (рекурсивно, когда элементы `Eq`). `Ord` реализован для: всех числовых типов, `char` `string` и
`cons-cell<A,B>` (лексикографически, когда элементы `Ord`).

Имена методов не совпадают со встроенными операторами (`= /= < <= > >=`) и `eq`/`lt`, потому что встроенные нельзя
переопределить, и каждая реализация делегирует им. Сами скалярные операторы сравнения — встроенные методы каждого
типа получателя ([Числа](numbers.md), [Строки и символы](collections.md)). Внутри ограничения записанные операторы
читаются как методы трейтов (глава 3).

## 3. Арифметические трейты (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Слой, через который обобщённый код может требовать «тип, который можно складывать». **Арифметика над конкретными
типами использует встроенные операторы** ([Числа](numbers.md)) и через этот слой не проходит.

```lisp
(deftrait Add () (add (self (other Self)) Self))
(deftrait Sub () (sub (self (other Self)) Self))
(deftrait Mul () (mul (self (other Self)) Self))
(deftrait Div () (:type Output) (div (self (other Self)) Output))
(deftrait Rem () (remainder (self (other Self)) Self))
(deftrait Bits ()
  (bit-and (self (other Self)) Self)
  (bit-or  (self (other Self)) Self)
  (bit-xor (self (other Self)) Self)
  (bit-not (self) Self)
  (shift (self (count int)) Self))                 ; расстояние всегда int (как у ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; методов нет; сочетание пяти
```

**Внутри ограничения можно писать операторы.** Когда получатель — переменная типа, связанная `:where`, операторы
читаются как методы трейтов (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`, `logand`→`bit-and`,
`=`→`equals`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

Метод трейта не называется `+`, потому что `+` — имя встроенного метода, а `impl` отказывается его переопределять
(`cannot redefine built-in method`). `Neg` нет: `(- x)` раскрывается в `(- (- x x) x)`, так что хватает `Sub`.

Реализованы для: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` — для всех числовых типов (кроме `complex`), `Bits` — для всех
целых типов и `int`.


**`Div` называет тип результата ассоциированным типом `Output`.** Как в CL, `/` на двух целых — это
`rational`, поэтому `Output` у `int` — `rational`, а у любого другого числового типа — сам этот тип.
Поскольку `Output` у типов разный, `Number` не включает `Div` (список супертрейтов должен закреплять
каждый ассоциированный тип). Обобщённая функция, которая делит, принимает тип результата через
`(:where (Div T (Output R)))` или требует деления, замкнутого в `T`, через `(:where (Div T (Output T)))`
(тогда `int` передать нельзя).

## 4. Трейт `Push` (добавление элементов)

```lisp
(deftrait Push ()
  (:type Item)
  (push (self (x Item)) ()))
```

Коллекция, в которую можно добавлять элементы по одному. Обобщённая функция требует «тип, в который
можно добавить `T`», через `(:where (Push C (Item T)))` и добавляет в любую такую коллекцию одним и
тем же `(push c x)`.

| Тип | Что делает `push` |
|---|---|
| `Vector<T>` | Добавляет в конец |
| `Deque<T>` | Добавляет в конец (`push-back`) |
| `Array<T>` | Добавляет по указателю заполнения (`push-extend`). Без указателя заполнения — panic |
| `HashSet<T>` | Помещает в множество (`insert`). Является `Push`, только когда тип элементов реализует `Hash` |

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

Собственный тип, реализующий `Push`, можно передавать тем же обобщённым функциям. Он может быть и
объектом трейта, как в `:dyn Push<int>`.

## 5. Трейт `Pop` (извлечение элементов)

```lisp
(deftrait Pop ()
  (:type Item)
  (pop (self) Option<Item>))
```

Коллекция, из которой можно извлечь элемент, добавленный `push` последним. `none`, если она пуста.
Обобщённая функция требует её через `(:where (Pop C (Item T)))`.

| Тип | Что делает `pop` |
|---|---|
| `Vector<T>` | Извлекает последний элемент |
| `Deque<T>` | Извлекает последний элемент (`pop-back`) |
| `Array<T>` | Извлекает элемент прямо перед указателем заполнения. Без указателя заполнения — panic |

У `HashSet<T>` нет «последнего элемента», поэтому он `Push`, но не `Pop`.

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

Как и `Push`, его может реализовать собственный тип, и он может быть трейт-объектом, как в
`:dyn Pop<int>`.
