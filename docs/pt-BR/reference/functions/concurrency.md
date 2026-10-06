<!-- translated-from: docs/ja/reference/functions/concurrency.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# Tarefas e canais

O vocabulário das tarefas (threads leves). `task` e `thread`, que as iniciam, e `select`, que espera várias
coisas, são formas especiais e estão na [Referência de sintaxe](../syntax.md#12-concorrência-tarefas). Este
capítulo trata do resto: tipos, métodos e funções.

As tarefas são **cooperativas**: uma tarefa só alterna nos pontos que você escreve. As tarefas são executadas
ao mesmo tempo em `TYPELISP_THREADS` threads de SO (no `typl`, só as tarefas compiladas vão para outras
threads). Onde elas alternam e como isso difere do Go está em
[Referência de sintaxe 12.5](../syntax.md#125-onde-as-tarefas-alternam) e
[12.7](../syntax.md#127-diferenças-em-relação-ao-go).

## 1. `Task<T>` — handles de tarefas

| Nome | Uso | Tipo | Significado |
|---|---|---|---|
| `wait` | `(wait t)` | `(Task<T>)→T` | Espera a conclusão e devolve seu valor |

```lisp
(let ((t1 (task (work 7))))
  (wait t1))
```

- **Você pode fazer `wait` quantas vezes quiser** (o valor fica em cache). Ao contrário do `JoinHandle::join` do
  Rust, ele não consome o handle, então pode-se esperar a partir de vários lugares.
- **Uma tarefa é executada mesmo que você nunca faça `wait`.** Descartar o handle não a para.
- É um valor comum, então pode ir em um `Vector<Task<()>>`.
- **Quando a tarefa principal termina, o processo termina** (como no Go). As outras tarefas em execução são
  interrompidas, e a limpeza de `unwind-protect` não é executada, porque isso é a saída do processo, não um
  desenrolar da pilha.

## 2. `Chan<T>` — canais

| Nome | Uso | Tipo | Significado |
|---|---|---|---|
| `Chan::new` | `(the Chan<int> (Chan::new 0))` | `(int)→Chan<T>` | Um canal de capacidade `n`. `0` é um encontro (sem buffer) |
| `send` | `(send ch v)` | `(Chan<T>,T)→()` | Espera até haver espaço e então entrega o valor |
| `recv` | `(recv ch)` | `(Chan<T>)→Option<T>` | Espera até chegar um valor. `none` quando está fechado e vazio |
| `close` | `(close ch)` | `(Chan<T>)→()` | Fecha-o |
| `len` | `(len ch)` | `(Chan<T>)→int` | Quantos valores há agora no buffer |
| `cap` | `(cap ch)` | `(Chan<T>)→int` | A capacidade |

**O argumento de tipo é dado com `the`** (escrito do mesmo jeito que `(the Vector<i32> (Vector::new))`).
**A capacidade sempre tem de ser escrita**: os dois casos que o Go escreve como `make(chan int)` e
`make(chan int, 16)` são escritos `(Chan::new 0)` e `(Chan::new 16)`.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n) (send ch (* i 2)))
  (close ch))

(let ((ch (the Chan<int> (Chan::new 2))))
  (task (produce ch 5))
  (doiter (v ch) (println "~a" v)))       ; o for v := range ch do Go
```

- **Um `Chan<T>` é seu próprio iterador** (implementa `Iter`). `recv` devolve um `Option<T>`, o mesmo tipo que
  `Iter::next`, então `doiter` e `map`/`filter`/`foldl` funcionam sobre ele como estão.
- **`send` em um canal fechado causa panic**, e **um segundo `close` também** (ambos como no Go). São bugs do
  programa, não falhas recuperáveis, então não são `Result`.
- **Fechar um canal em que uma tarefa espera para fazer `send` faz essa tarefa entrar em panic** (a regra do
  Go).
- `recv` em um canal fechado devolve o que restou no buffer e, quando ele fica vazio, continua devolvendo
  `none`.
- `close` é resolvido pelo tipo do receptor, então é algo diferente do `close` do trait `Stream`. `Chan<T>` não
  implementa `Stream`.
- **Uma capacidade negativa causa panic** (não é arredondada silenciosamente para 0).

## 3. `yield` / `sleep` — ceder a vez

| Nome | Uso | Tipo | Significado |
|---|---|---|---|
| `yield` | `(yield)` | `()→()` | Abre mão do resto da sua vez (o `runtime.Gosched` do Go) |
| `sleep` | `(sleep 0.5)` | `(f64)→()` | Para **só aquela tarefa**. As outras continuam em execução |

`sleep` para uma tarefa, não uma thread. Só quando nenhuma tarefa pode ser executada é que ele entra em um
`sleep` do SO até o prazo mais próximo. `(sleep 0.0)` é o "ceder por 0 segundos" do CL.

Como no CL, `sleep` recebe **segundos**. Os inteiros não são convertidos automaticamente para ponto
flutuante, então o `(sleep 1)` do CL é escrito aqui `(sleep 1.0)`. Um valor negativo ou NaN causa panic.

## 4. `WaitGroup` — esperar N conclusões

| Nome | Uso | Tipo | Significado |
|---|---|---|---|
| `WaitGroup::make` | `(the WaitGroup (WaitGroup::make))` | `()→WaitGroup` | Um grupo sem nada pendente |
| `add` | `(add wg 1)` | `(WaitGroup,int)→()` | Soma ao contador. Faça isso antes de o trabalho começar |
| `done` | `(done wg)` | `(WaitGroup)→()` | Um terminou. Ao chegar a 0, todos os que esperam são liberados |
| `wait` | `(wait wg)` | `(WaitGroup)→()` | Espera até chegar a 0. A partir de quantas tarefas forem |

`(wait wg)` e `(wait task)` são resolvidos pelo tipo do receptor, então convivem com o mesmo nome. Se o
contador ficar abaixo de 0, ocorre panic (`done` chamado vezes demais, ou um `add` negativo). Como no Go, um
grupo que voltou a 0 pode ser usado de novo começando por `add`. Nenhuma atualização se perde mesmo quando as
tarefas são executadas em threads de SO diferentes.

**Com `Task<T>` disponível, ele é menos necessário do que no Go**: `(doiter (t tasks) (wait t))` muitas vezes
basta. É uma ferramenta para trabalho que cresce dinamicamente, ou para quando você não quer guardar os
handles.

```lisp
;; fan-in: iniciar uma tarefa por entrada e reuni-las (esta linguagem não tem canais nil)
(defvar (left i32) 0)
(defun drain ((in Chan<i32>) (out Chan<i32>)) ()
  (doiter (v in) (send out v))
  (setf left (- left 1))
  (if (eq left 0) (close out) ()))
```

## 5. `after` — um canal que entrega após um tempo

| Nome | Uso | Tipo | Significado |
|---|---|---|---|
| `after` | `(recv (after 0.5))` | `(f64)→Chan<()>` | Um canal que entrega um valor depois de `sec` segundos |

O `time.After` do Go. Pode ser escrito como está no ramo de tempo limite de `select`
([Referência de sintaxe 12.3](../syntax.md#123-select--esperar-várias-operações-de-canal-ao-mesmo-tempo)).
Sua capacidade é 1, então a tarefa que envia pode terminar mesmo que ninguém receba.

## 6. `Mutex<T>` — exclusão mútua para dados compartilhados

| Nome | Uso | Tipo | Significado |
|---|---|---|---|
| `Mutex::make` | `(the Mutex<i32> (Mutex::make 0))` | `(T)→Mutex<T>` | Um mutex destravado contendo `v` |
| `lock` | `(lock m)` | `(Mutex<T>)→()` | Pega a trava (espera) |
| `unlock` | `(unlock m)` | `(Mutex<T>)→()` | Libera-a. Panic se não estiver travado |
| `with-lock` | `(with-lock (x m) body...)` | Macro | Trava, vincula o conteúdo a `x`, executa `body` e **sempre** libera |

```lisp
(let ((counter (the Mutex<i32> (Mutex::make 0))))
  (with-lock (n counter) (setf n (+ n 1)))
  (with-lock (n counter) n))                     ; => 1
```

- **`x` não é uma cópia do valor, mas um "lugar"** (`symbol-macrolet`). `(setf x 42)` muda o conteúdo do
  mutex.
- `with-lock` libera com `unwind-protect`, então a trava é liberada qualquer que seja a forma de sair do corpo:
  conclusão normal, `throw`, `panic` ou `break`/`return`/`return-from`.
- **Entrar de novo gera um deadlock** (não um panic). O escalonador relata que "nada pode avançar" para uma
  tarefa presa na própria trava.
- **`m::v` mexe no conteúdo de fora da trava**, o que é indefinido no sentido de que outra tarefa pode estar
  no meio de uma mudança. É a mesma posição do `sync.Mutex` do Go: em uma linguagem sem posse nem
  verificação de empréstimos, não dá para construir uma garantia estática como o `MutexGuard`.

## 7. `Thread<T>` — threads de SO dedicadas

O handle devolvido por `(thread (f args...))`
([Referência de sintaxe 12.2](../syntax.md#122-thread--iniciar-uma-tarefa-em-uma-thread-de-so-dedicada)). A
contrapartida de `Task<T>`.

| Nome | Uso | Tipo | Significado |
|---|---|---|---|
| `join` | `(join th)` | `(Thread<T>)→T` | Espera a conclusão e devolve o valor (para a **tarefa** que chama. Pode ser chamado quantas vezes quiser; o valor fica em cache) |
| `Thread::spawn` | `(Thread::spawn (lambda () int 42))` | `((fn () T))→Thread<T>` | A versão em forma de função de `(thread (f))` (o `std::thread::spawn` do Rust) |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | O número da thread de SO em execução. Único dentro do processo, sem significado além de "é a mesma thread?" |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | O número de threads que a máquina consegue executar ao mesmo tempo (o padrão de `TYPELISP_THREADS`). Panic se o SO não responder |

```lisp
(defffi (c-usleep "usleep") (u32) i32)
(defun sleepy ((us int)) int
  (progn (unsafe (c-usleep (as u32 us))) us))    ; uma função C bloqueante
(let ((th (thread (sleepy 500000))))
  ...                                            ; enquanto isso, as outras tarefas seguem em frente
  (join th))                                     ; => 500000
```

- Chamar uma função C bloqueante (`defffi`) para só aquela thread.
- Um `task` dentro de uma `thread` é executado como uma tarefa comum em outras threads.
- Também pode ser usado no `typl`. Ao interpretar, `(thread (f ...))` e `Thread::spawn` compilam na hora a
  função a executar e então a executam na thread dedicada. Uma `lambda` que se refere a variáveis locais de fora
  não pode ser compilada sozinha e causa panic
  ([Referência de sintaxe 12.2](../syntax.md#122-thread--iniciar-uma-tarefa-em-uma-thread-de-so-dedicada)).
  Uma `lambda` criada dentro de uma função compilada pode ser passada.

## 8. O que não existe

- **`Atomic`**. `Mutex` basta.
- **Variáveis locais de tarefa** (o Go também não as tem).
- **Canais nil**. O motivo e a alternativa estão em
  [Referência de sintaxe 12.7](../syntax.md#127-diferenças-em-relação-ao-go).
