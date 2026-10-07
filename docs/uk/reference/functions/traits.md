<!-- translated-from: docs/ja/reference/functions/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Стандартні трейти

Трейти для ітерації, порівняння та арифметики. Інші стандартні трейти описано у власних розділах: `Hash`
([HashTable](collections.md#4-hashtablekv)), `Error`
([Типи помилок](option-result.md#3-типи-помилок-і-трейт-error)), `print-object`
([Друк](printing.md#5-print-object-представлення-для-друку-для-кожного-типу)), а також трейти потоків і
`Pathish` ([Потоки та файли](streams-files.md)). Які типи що реалізують, наведено в розділі
[Типи](../types.md). Як визначати трейти, описано в
[Довіднику із синтаксису](../syntax.md#39-deftrait--impl--трейти).

## 1. Трейт `Iter` та ітерація

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` реалізують `Iter` через `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` відповідно (ітератор отримують через `(iter collection)`).
`Chan<T>` сам є `Iter` (`recv` відіграє роль `next`; [Канали](concurrency.md#2-chant--канали)).
Списки `Sexpr` не реалізують `Iter` (типи їхніх елементів неоднорідні). Якщо реалізувати `Iter`
для власного типу, його можна обходити через `doiter` як є і передавати в
[функції над послідовностями](sequences.md#4-функції-над-послідовностями-для-iter).

## 2. `Eq` / `Ord` (порівняння)

Вони відповідають `PartialEq`/`PartialOrd` з Rust (названі `Eq`/`Ord`). Їх використовують в обмеженнях
`where` узагальнених функцій, щоб вимагати, що типи елементів можна порівнювати (`sort`/`member`/`assoc`
тощо).

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; must be implemented
  (not-equals ((self Self) (other Self)) bool             ; default implementation
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; inherits from Eq
  (less ((self Self) (other Self)) bool)                  ; must be implemented
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
```

Щоб реалізувати `Eq`, пишуть лише `equals`, а для `Ord` — лише `less`. Решту заповнюють типові
реалізації. `Ord` успадковує `Eq`, тож `impl Eq X` потрібен перед `impl Ord X`.

Кожен метод трейта можна викликати як функцію як є (всередині обмеження `where (Eq A)`/`(Ord A)` або
для конкретного типу, що його реалізує):

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | Чи рівні вони (`==` у Rust) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | Чи не рівні вони (`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` реалізовано для: усіх числових типів (від `i8` до `u32` / `f32` / `f64` / `int` / `ratio`), `bool`
`char` `string` `symbol` `complex`, `Sexpr` (`eq`, тобто тотожність; використовується в візерунках
значень `match`) і `cons-cell<A,B>` (рекурсивно, коли елементи є `Eq`). `Ord` реалізовано для:
усіх числових типів, `char` `string` і `cons-cell<A,B>` (лексикографічно, коли елементи є
`Ord`).

Назви методів не перетинаються із вбудованими операторами (`= /= < <= > >=`) чи `eq`/`lt`, бо
вбудовані не можна перевизначити, і кожна реалізація делегує їм. Самі оператори скалярного порівняння —
це вбудовані методи кожного типу одержувача ([Числа](numbers.md),
[Рядки та знаки](collections.md)). Усередині обмеження запис операторів читається як методи трейта
(розділ 3).

## 3. Арифметичні трейти (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Шар, що дає змогу узагальненому коду вимагати «тип, який можна додавати». **Арифметика над конкретними
типами використовує вбудовані оператори** ([Числа](numbers.md)) і через цей шар не проходить.

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
  (shift ((self Self) (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Div Rem Ord))               ; no methods; a combination of six
```

**Всередині обмеження можна писати оператори.** Коли одержувач — змінна типу, обмежена через `where`,
оператори читаються як методи трейта (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`,
`logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

Метод трейта не названо `+`, бо `+` — це назва вбудованого методу, а `impl` відмовляється його
перевизначати (`cannot redefine built-in method`). `Neg` немає: `(- x)` розгортається в
`(- (- x x) x)`, тож `Sub` достатньо.

Реалізовано для: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` для всіх числових типів (крім `complex`) і
`Bits` для всіх цілих типів та `int`.
