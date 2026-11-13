<!-- translated-from: docs/ja/reference/functions/traits.md @ cedb434055399f2f83e17dcdaa55007fe4fc9dad -->
# Стандартні трейти

Трейти для ітерації, порівняння, арифметики, додавання та вилучення елементів. Інші стандартні трейти описано у власних розділах: `Hash`
([HashTable](collections.md#4-hashtablekv)), `Error`
([Типи помилок](option-result.md#3-типи-помилок-і-трейт-error)), `print-object`
([Друк](printing.md#5-print-object-представлення-для-друку-для-кожного-типу)), а також трейти потоків і
`Pathish` ([Потоки та файли](streams-files.md)). Які типи що реалізують, наведено в розділі
[Типи](../types.md). Як визначати трейти, описано в
[Довіднику із синтаксису](../syntax.md#39-deftrait--impl--трейти).

## 1. Трейт `Iter` та ітерація

```lisp
(deftrait Iter ()
  (:type Item)
  (defmethod next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` реалізують `Iter` через `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` відповідно (ітератор отримують через `(iter collection)`).
`Chan<T>` сам є `Iter` (`recv` відіграє роль `next`; [Канали](concurrency.md#2-chant--канали)).
Списки `Sexpr` не реалізують `Iter` (типи їхніх елементів неоднорідні). Якщо реалізувати `Iter`
для власного типу, його можна обходити через `doiter` як є і передавати в
[функції над послідовностями](sequences.md#4-функції-над-послідовностями-для-iter).

## 2. `Equal` / `Ord` (порівняння)

`Equal` — трейт, яким користується `equal`; він відповідає `PartialEq` у Rust. `Ord` відповідає
`PartialOrd` у Rust. Їх використовують в обмеженнях `:where` узагальнених функцій, щоб вимагати, аби тип
елементів можна було порівнювати.

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; обов'язковий
(deftrait Ord (Equal)                                     ; успадковує Equal
  (defmethod less (self (other Self)) bool)                  ; обов'язковий
  (defmethod less-equal (self (other Self)) bool (not (less other self)))
  (defmethod greater (self (other Self)) bool (less other self))
  (defmethod greater-equal (self (other Self)) bool (not (less self other))))
```

Щоб реалізувати `Equal`, пишуть `equal`, а для `Ord` лише `less`. Решту `Ord` доповнюють реалізації за
замовчуванням. `Ord` успадковує `Equal`, тож перед `impl Ord X` потрібен `impl Equal X`.

`(equal a b)` викликає реалізацію `Equal` для типу `a`. Написати його для типу, який не реалізує `Equal`, —
помилка типу. Для порівняння за ідентичністю використовують `eq`/`eql`, за структурою — `equalp`. Ці три
застосовні до значення будь-якого типу.

Кожен метод трейта можна викликати прямо як функцію (усередині обмеження `where (Equal A)`/`(Ord A)` або
для конкретного типу, що його реалізує):

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` where `Equal A` | Чи рівні вони (`==` у Rust) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Equal` реалізовано для: усіх числових типів (від `i8` до `u32` / `f32` / `f64` / `int` / `ratio`), `bool`
`char` `string` `symbol` `complex`, `Sexpr`, `cons-cell<A,B>` і кортежів (поелементно, якщо елементи
`Equal`), а також `Option<T>` і `Result<T,E>` (якщо це той самий варіант і вміст `equal`). Для вбудованих
типів власний вбудований метод `equal` типу і є реалізацією `Equal`. Числа порівнюються через `=`, рядки —
за вмістом з урахуванням регістру. `Ord` реалізовано для: усіх числових типів, `char` `string` і
`cons-cell<A,B>` (лексикографічно, якщо елементи `Ord`).

`equal` списку (`Sexpr`) — це `equal` з CL: cons-комірки порівнюються за структурою, рядки — за вмістом,
будь-який інший атом — через `eql`. Це порівняння обходить значення під час виконання і не викликає жодних
методів. Тому структура всередині списку, як і в CL, порівнюється за ідентичністю, хоч би який `impl Equal`
мав її тип.

Імена методів `Ord` не перетинаються з вбудованими операторами (`< <= > >=`) і `lt`, бо вбудоване не можна
перевизначити, а кожна реалізація делегує їм. Самі скалярні оператори порівняння — вбудовані методи для
кожного типу отримувача ([Числа](numbers.md), [Рядки й символи](collections.md)). Усередині обмеження
написаний оператор читається як метод трейта (розділ 3). `=` стає `equal`, а `/=` — `(not (equal a b))`.

## 3. Арифметичні трейти (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Шар, що дає змогу узагальненому коду вимагати «тип, який можна додавати». **Арифметика над конкретними
типами використовує вбудовані оператори** ([Числа](numbers.md)) і через цей шар не проходить.

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
  (defmethod shift (self (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; методів немає; поєднання п'яти
```

**Всередині обмеження можна писати оператори.** Коли одержувач — змінна типу, обмежена через `:where`,
оператори читаються як методи трейта (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`,
`logand`→`bit-and`, `=`→`equal`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

Метод трейта не названо `+`, бо `+` — це назва вбудованого методу, а `impl` відмовляється його
перевизначати (`cannot redefine built-in method`). `Neg` немає: `(- x)` розгортається в
`(- (- x x) x)`, тож `Sub` достатньо.

Реалізовано для: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` для всіх числових типів (крім `complex`) і
`Bits` для всіх цілих типів та `int`.


**`Div` називає тип результату асоційованим типом `Output`.** Як у CL, `/` на двох цілих — це
`rational`, тому `Output` у `int` — `rational`, а в будь-якого іншого числового типу — сам цей тип.
Оскільки `Output` у типів різний, `Number` не містить `Div` (список супертрейтів має закріплювати кожен
асоційований тип). Узагальнена функція, що ділить, приймає тип результату через
`(:where (Div T (Output R)))` або вимагає ділення, замкненого в `T`, через `(:where (Div T (Output T)))`
(тоді `int` передати не можна).

## 4. Трейт `Push` (додавання елементів)

```lisp
(deftrait Push ()
  (:type Item)
  (defmethod push (self (x Item)) ()))
```

Колекція, до якої можна додавати елементи по одному. Узагальнена функція вимагає «тип, до якого
можна додати `T`», через `(:where (Push C (Item T)))` і додає до будь-якої такої колекції тим самим
`(push c x)`.

| Тип | Що робить `push` |
|---|---|
| `Vector<T>` | Додає в кінець |
| `Deque<T>` | Додає в кінець (`push-back`) |
| `Array<T>` | Додає за покажчиком заповнення (`push-extend`). Panic без покажчика заповнення |
| `HashSet<T>` | Кладе в множину (`insert`) |

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

Власний тип, що реалізує `Push`, можна передавати тим самим узагальненим функціям. Він може бути й
об'єктом трейту, як у `:dyn Push<int>`.

## 5. Трейт `Pop` (вилучення елементів)

```lisp
(deftrait Pop ()
  (:type Item)
  (defmethod pop (self) Option<Item>))
```

Колекція, з якої можна вилучити елемент, доданий `push` останнім. `none`, якщо вона порожня.
Узагальнена функція вимагає її через `(:where (Pop C (Item T)))`.

| Тип | Що робить `pop` |
|---|---|
| `Vector<T>` | Вилучає останній елемент |
| `Deque<T>` | Вилучає останній елемент (`pop-back`) |
| `Array<T>` | Вилучає елемент безпосередньо перед покажчиком заповнення. Без покажчика заповнення — panic |

У `HashSet<T>` немає «останнього елемента», тож він `Push`, але не `Pop`.

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

Як і `Push`, його може реалізувати власний тип, і він може бути трейт-об'єктом, як у
`:dyn Pop<int>`.
