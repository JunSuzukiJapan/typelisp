<!-- translated-from: docs/ja/reference/errors.md @ b550ae4adbf0e3f78566a6c1f79d671a5d08627e -->
# Mensajes de error

Qué significan los principales mensajes de error de `typl` y cómo corregirlos.

## 1. Cómo leer un error

Los errores se escriben en la salida de error estándar con esta forma:

```text
error: archivo:línea:columna: clase: mensaje
```

La `clase` indica cuándo se encontró el error.

| Clase | Cuándo | Significado |
|---|---|---|
| `type error` | Antes de ejecutar (al comprobar) | Un error en tipos o nombres. Esa forma no se ejecuta |
| (sin clase) | Al leer o al comprobar | Un error de sintaxis como paréntesis desequilibrados, o un nombre que no se encuentra |
| `panic` | Durante la ejecución | Un fallo irrecuperable. El programa se detiene tras ejecutar la limpieza de `unwind-protect` |

Las líneas que empiezan por `warning:` son avisos, y el procesamiento continúa.

`archivo:línea:columna` señala la expresión con el error. Para un error de ejecución que ocurre dentro de
una función de la biblioteca estándar, señala el lugar donde el programa llamó a esa función. Algunos
errores no tienen posición (como `error: panic: ...`).

Ejemplo:

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

Significa que la expresión de la línea 1, columna 24 de `main.typl` era un `string` donde se esperaba un
`i32`.

## 2. Errores al comprobar

Errores encontrados antes de ejecutar. La forma no se ejecuta hasta que se corrigen.

### 2.1 Tipos

| Mensaje | Significado y corrección |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | Hay una expresión de tipo `U` donde se necesita el tipo `T`. No hay conversiones implícitas; para números, convierte con `(as T x)`. `int` e `i32` también son tipos distintos |
| ``integer literal 300 is out of range for u8 (0..=255)`` | El literal no cabe en el tipo. Si quieres recortarlo, escribe `(as u8 300)` |
| ``unknown type `foo`: no type of that name is visible here. ...`` | No hay ningún tipo con ese nombre. Define el tipo antes de la primera forma que lo use (los tipos no tienen declaración anticipada). Si querías una variable de tipo, escríbela en una posición de declaración como `<foo>` tras el nombre de la función ([Referencia de sintaxis 3.6](syntax.md#36-defstruct--estructuras-tipos-definidos-por-el-usuario)) |
| ``cannot infer type argument `t` for `vector::new` `` | No se puede determinar un argumento de tipo. Escribe el tipo con `the`, como en `(the Vector<int> (Vector::new))` |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | El `match` no trata todas las variantes. Añade ramas para las variantes que faltan, o una rama `_` |
| ``type `pt` does not implement trait `eq` required by `:where` clause on type parameter `a` `` | La función exige un trait que el tipo que pasaste no implementa. Escribe `(impl Eq pt ...)` ([Traits estándar](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | Se pasó un valor de un tipo que no implementa el trait donde se espera un `:dyn`. Escribe el `impl` |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | Se escribió un nombre de trait donde va un tipo. Escribe `:dyn Error` |
| ``if: (if cond then else)`` | El `if` tiene una forma incorrecta. `if` exige una rama else. Cuando no la necesites, usa `when` |

### 2.2 Nombres

| Mensaje | Significado y corrección |
|---|---|
| `no such function: bar` | No hay ninguna función ni método con ese nombre. Comprueba la ortografía |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | Los métodos se seleccionan por el tipo del primer argumento. Existe un método con ese nombre, pero no para el tipo del primer argumento (`int` aquí). El final del mensaje enumera los tipos que tienen el método (para un método escrito para una instancia, la instancia, como `box<int>`) |
| ``no method `twice` for type `box<string>` (the first parameter of the function this value is expected to be, which selects the method); `twice` is a method of `box<int>` `` | Un método pasado como valor se elige por el tipo del primer parámetro del tipo de función al que se pasa. Ese tipo (aquí `box<string>`) no tiene un método con ese nombre. El final del mensaje enumera los tipos que lo tienen |
| `unbound variable: y` | No hay ninguna variable con ese nombre. Comprueba la ortografía y el alcance del enlace (¿se usa fuera de su `let`?) |
| ``cannot define variable `f`: a function of that name already exists here — a name has one meaning, whether a function, a variable or a macro`` | Se dio el mismo nombre a dos de una función, una variable global y una macro en un mismo módulo. Cambia el nombre de una de ellas ([Referencia de sintaxis 3.10](syntax.md#310-module--use--espacios-de-nombres)) |
| ``` `point` has no static method `new`; a struct's methods are written in its `impl`, ...``` | Para una estructura no se genera ningún `new`. Crea el valor con `make-instance` o escribe un `new` en `impl` ([Referencia de sintaxis 3.5](syntax.md#35-impl--métodos-de-un-tipo)) |
| ``defmethod is gone — write methods in the type's `impl`: ...`` | No existe `defmethod`. Los métodos se escriben en el `impl` del tipo ([Referencia de sintaxis 3.5](syntax.md#35-impl--métodos-de-un-tipo)) |
| ``defstruct: (:constructor ...) is gone — ...`` | No existe la opción `:constructor`. El constructor se escribe en `impl` como método estático |
| ``method box::m: `box` has a `m` for every instantiation already, so an `impl` of `box<int>` cannot define its own ...`` | Un método con el mismo nombre se escribió tanto en un `impl` cuyos parámetros de tipo son variables (`Box<T>`) como en el `impl` de una instancia (`Box<int>`), en cualquier orden. Deja solo uno ([Referencia de sintaxis 3.5](syntax.md#35-impl--métodos-de-un-tipo)) |
| ``impl show box<int>: `box` implements `show` for every instantiation already, ...`` | Se implementó el mismo trait para `Box<T>` y para `Box<int>`. Deja solo uno |
| ``use: unresolved `nosuch` `` | No se encuentra el módulo indicado en `use`. Para ver cómo corresponden los nombres de archivo a las rutas de módulo, consulta [Referencia de sintaxis 3.11](syntax.md#311-archivos-y-módulos-proyectos-de-varios-archivos) |
| `unresolved path: c::hidden` | El módulo existe, pero el nombre no, o no es visible porque le falta `pub` |
| `circular module dependency: a -> b -> a` | Los módulos se usan mutuamente con `use`. Mueve la parte compartida a un módulo aparte |
| ``return-from: no enclosing block named `nope` `` | Ningún `block` con el nombre dado a `return-from` lo envuelve. El bloque de una función solo se puede usar dentro de esa función |

### 2.3 Llamadas

| Mensaje | Significado y corrección |
|---|---|
| `f: expected 1 argument(s), got 2` | El número de argumentos no coincide |
| `f: unknown keyword argument :b` | Se pasó un argumento de palabra clave que la función no tiene |
| `circle: expected 1 field(s), got 2` | El número de valores pasados a una variante de un enum no coincide con el número de campos de esa variante |
| ``make-instance point: slot `y` is not given and has no default`` | No se dio a `make-instance` un campo que no tiene valor por defecto |
| ``make-instance point: no slot `z` (its slots are `x`, `y`)`` | Se dio un campo que la estructura no tiene |
| ``make-instance geo::pt: slot `secret` is not `pub`, so it can be given only inside module `geo` `` | Se dio un campo que no es `pub` fuera del módulo que define el tipo. Pide a ese módulo un constructor `pub` |
| ``` `box::make` is written for `box<int>`, `box<string>`; the type the call is expected to return picks one, ...``` | Los `impl` de varias instancias tienen un método estático con ese nombre y nada decide a cuál llamar. Indica el tipo esperado, como en `(the Box<int> (Box::make))` |
| ``compile: `box::twice` is written for more than one instantiation of `box`; name one: ...`` | Se pasó a `compile`, `trace`, `documentation` u otros un método escrito por instancia usando solo el nombre del tipo. Escribe la instancia, como en `box<int>::twice` |
| ``setf: cannot assign to constant `k` `` | Se asignó a un nombre definido con `defconstant`. Si tiene que cambiar, usa `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | Una función declarada con `defsignature` no está definida |
| ``format: ~/nosuch/ calls what (nosuch arg colon at) with arg a `string` would: no such function: nosuch`` | La llamada `(nosuch arg colon at)` escrita en ese lugar no se puede resolver. Define un método del tipo del argumento o una función ([Directivas de formato, capítulo 5](functions/format.md#5-name)) |
| ``format: ~/brief/ can be handed an element of a list argument, whose type is only `Sexpr` — ...`` | Se escribió `~/name/` donde recibe los elementos de una lista, como dentro de `~{`. El tipo de los elementos no puede decidir a qué llamar |

## 3. Errores de lectura

| Mensaje | Significado y corrección |
|---|---|
| `unexpected end of input while reading a list` | Falta un paréntesis de cierre. La posición señala dónde terminó la lectura (como el final del archivo), así que busca el paréntesis de apertura |

## 4. Errores en tiempo de ejecución (panic)

| Mensaje | Significado y corrección |
|---|---|
| `panic: divide by zero` | División por cero con enteros o racionales. La división por cero en coma flotante no provoca un panic; da `inf`/`NaN` |
| `panic: unwrap: called on none` | Se aplicó `unwrap` a `none`. Trata el caso `none` con `match` o `unwrap-or` |
| `panic: Vector: index 5 out of bounds` | Un índice fuera de rango. Comprueba la longitud con `len`, o usa una función que devuelva `none` fuera de rango (`nth`, `pop`, etc.) |
| `panic: an integer argument does not fit a fixnum` | Se pasó un `int` que no cabe en 63 bits a un argumento que recibe un índice o una cuenta |
| `throw: no enclosing (catch 'oops) for this throw` | Se ejecutó un `throw` sin ningún `catch` envolvente con la misma etiqueta |
| `panic: <message>` | El programa llamó a `(panic "<message>")`. Un `assert` fallido da `assertion failed: ...` |

Un `panic` detiene el proceso entero aunque ocurra dentro de una tarea
([Referencia de sintaxis 12.4](syntax.md#124-interacción-con-otras-funciones)). Expresa los fallos de los
que quieras recuperarte con `Result` ([capítulo 9 de la Referencia de sintaxis](syntax.md#9-política-de-manejo-de-errores)).

## 5. Avisos

| Mensaje | Significado |
|---|---|
| ``warning: redefining function `f` `` | Se volvió a definir una función con el mismo nombre. Surte efecto la definición posterior. Aparece normalmente al corregir una definición en el REPL |
