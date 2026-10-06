<!-- translated-from: docs/ja/reference/functions/traits.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Стандартные трейты

Трейты итерации, сравнения и арифметики. Остальные стандартные трейты описаны в собственных главах: `Hash`
([HashTable](collections.md#4-hashtablekv)), `Error` ([Типы ошибок](option-result.md#3-типы-ошибок-и-трейт-error)),
`print-object` ([Печать](printing.md#5-print-object-печатное-представление-по-типу)), а также трейты потоков и
`Pathish` ([Потоки и файлы](streams-files.md)). Какие типы что реализуют, указано в [Типах](../types.md). Как
определять трейты, описано в [Справочнике по синтаксису](../syntax.md#39-deftrait--impl--трейты).

## 1. Трейт `Iter` и итерация

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` реализуют `Iter` соответственно через `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (итератор получают через `(iter коллекция)`). `Chan<T>` сам является `Iter`
(`recv` играет роль `next`; [Каналы](concurrency.md#2-chant--каналы)). Списки `Sexpr` не реализуют `Iter` (типы их
элементов неоднородны). Если реализовать `Iter` для собственного типа, его можно как есть обходить через `doiter` и
передавать [функциям последовательностей](sequences.md#4-функции-последовательностей-над-iter).

## 2. `Eq` / `Ord` (сравнение)

Соответствуют `PartialEq`/`PartialOrd` из Rust (названы `Eq`/`Ord`). Используются в ограничениях `where` обобщённых
функций, чтобы потребовать сравнимости типов элементов (`sort`/`member`/`assoc` и т. д.).

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; нужно реализовать
  (not-equals ((self Self) (other Self)) bool             ; реализация по умолчанию
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; наследуется от Eq
  (less ((self Self) (other Self)) bool)                  ; нужно реализовать
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
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
  (shift ((self Self) (count int)) Self))                 ; расстояние всегда int (как у ash)
(deftrait Number (Add Sub Mul Div Rem Ord))               ; без методов; сочетание шести
```

**Внутри ограничения можно писать операторы.** Когда получатель — переменная типа, связанная `where`, операторы
читаются как методы трейтов (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`, `logand`→`bit-and`,
`=`→`equals`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

Метод трейта не называется `+`, потому что `+` — имя встроенного метода, а `impl` отказывается его переопределять
(`cannot redefine built-in method`). `Neg` нет: `(- x)` раскрывается в `(- (- x x) x)`, так что хватает `Sub`.

Реализованы для: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` — для всех числовых типов (кроме `complex`), `Bits` — для всех
целых типов и `int`.
