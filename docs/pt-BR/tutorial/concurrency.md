<!-- translated-from: docs/ja/tutorial/concurrency.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Concorrência

No typelisp o trabalho concorrente é feito iniciando **tarefas** (threads leves), e as tarefas passam valores
umas às outras por meio de **canais**. O modelo é parecido com as goroutines e os canais do Go. Este capítulo
aborda, nesta ordem, como iniciar uma tarefa e obter seu resultado, os canais, `select`, a proteção de dados
compartilhados e as threads de SO dedicadas. Ele pressupõe que você leu [Fundamentos de tipos](types.md).

## 1. Iniciar uma tarefa e esperar seu resultado

`(task (função argumentos...))` inicia uma chamada de função como uma nova tarefa. Quem a inicia não espera e
segue em frente. O valor é um handle do tipo `Task<T>`; `(wait handle)` espera a tarefa terminar e devolve
seu resultado.

```lisp
(defun slow-square ((n int)) int
  (sleep 0.1)                  ; espera 0,1 segundo (só esta tarefa para)
  (* n n))

(let ((a (task (slow-square 3)))
      (b (task (slow-square 4))))
  (println "~a" (+ (wait a) (wait b))))    ; 25
```

- `task` só aceita a forma de uma chamada de função. Os argumentos são avaliados onde o `task` é escrito; na
  nova tarefa só a chamada em si é executada.
- Para executar várias expressões, crie uma `lambda` e chame-a na hora:
  `(task ((lambda () () (println "start") (work))))`
- Você pode chamar `wait` quantas vezes quiser. O resultado é lembrado.
- Uma tarefa é executada mesmo que você nunca faça `wait` nela.
- **Quando o trabalho principal termina, o programa termina.** As tarefas ainda em execução são
  interrompidas.

## 2. Passar valores por canais

Um canal `Chan<T>` é um caminho pelo qual as tarefas passam valores do tipo `T`. O argumento de `Chan::new` é
a capacidade (quantos valores ele pode guardar). Em um canal de capacidade 0, quem envia e quem recebe
esperam ambos até o outro lado estar presente.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n)
    (send ch (* i 10)))        ; enviar
  (close ch))                  ; não haverá mais envios

(let ((ch (the Chan<int> (Chan::new 0))))
  (task (produce ch 4))
  (doiter (v ch)               ; receber até ser fechado
    (println "got ~a" v)))
```

```
got 0
got 10
got 20
got 30
```

- `(send ch v)` envia. Se o canal estiver cheio, espera até haver espaço.
- `(recv ch)` recebe. Espera até chegar um valor. O resultado é um `Option<T>`; quando o canal está fechado e
  vazio, devolve `none`.
- Percorrer um canal com `doiter` continua recebendo valores até que ele seja fechado. Ele também pode ser
  passado diretamente para `map` ou `filter`.
- `send` em um canal fechado causa panic.

### Dividir o trabalho entre várias tarefas

Um padrão comum é montar um canal que transporta o trabalho e fazer vários workers (tarefas que processam o
trabalho) pegarem trabalhos dele. O worker que estiver livre pega o próximo trabalho, então mesmo quando
trabalhos lentos e rápidos se misturam, o trabalho se distribui naturalmente.

```lisp
(defstruct job
  (id int)
  (seconds f64))                  ; quanto tempo este trabalho leva

(defun worker ((name string) (jobs Chan<job>)) int
  (let ((count 0))
    (doiter (j jobs)              ; pega um trabalho por vez até jobs ser fechado
      (println "~a: start  job ~a (~as)" name j::id j::seconds)
      (sleep j::seconds)          ; trabalhando; enquanto isso, outros workers pegam os próximos
      (println "~a: finish job ~a" name j::id)
      (setf count (+ count 1)))
    count))                       ; quantos trabalhos este worker fez

(let* ((jobs (the Chan<job> (Chan::new 0)))
       (a (task (worker "A" jobs)))
       (b (task (worker "B" jobs)))
       (c (task (worker "C" jobs))))
  (send jobs (job::new 1 0.3))
  (send jobs (job::new 2 0.1))
  (send jobs (job::new 3 0.1))
  (send jobs (job::new 4 0.2))
  (send jobs (job::new 5 0.1))
  (send jobs (job::new 6 0.1))
  (close jobs)                    ; esse é todo o trabalho
  (println "A: ~a jobs, B: ~a jobs, C: ~a jobs" (wait a) (wait b) (wait c)))
```

```
A: start  job 1 (0.3s)
B: start  job 2 (0.1s)
C: start  job 3 (0.1s)
B: finish job 2
B: start  job 4 (0.2s)
C: finish job 3
C: start  job 5 (0.1s)
C: finish job 5
C: start  job 6 (0.1s)
A: finish job 1
B: finish job 4
C: finish job 6
A: 1 jobs, B: 2 jobs, C: 3 jobs
```

- A, B e C pegam cada um um dos três primeiros trabalhos.
- Depois de 0,1 segundo B e C ficam livres e pegam os trabalhos restantes. Enquanto A está ocupado com o lento
  trabalho 1, não pega trabalho novo.
- No fim, A cuidou de um trabalho, B de dois e C de três. Nada no programa diz qual worker pega qual trabalho.
- Fechar `jobs` termina o `doiter` de cada worker, as tarefas terminam e cada `wait` devolve sua contagem.

`jobs` é um canal de capacidade 0, então `send` espera até que algum worker pegue o trabalho. Com uma
capacidade maior, a tarefa principal poderia enfileirar trabalho sem esperar pelos workers.

## 3. `select`: esperar vários canais ao mesmo tempo

`select` executa a primeira de várias operações de canal que se tornar possível. `(after segundos)` é um canal
que entrega um valor depois que o tempo dado passa. Combinado com `select`, ele dá um tempo limite.

```lisp
(defun late-send ((ch Chan<string>) (sec f64)) ()
  (sleep sec)
  (send ch "done"))

(let ((ch (the Chan<string> (Chan::new 1))))
  (task (late-send ch 1.0))
  (select
    ((v (recv ch)) (println "~a" (unwrap v)))
    ((z (recv (after 0.2))) (println "timeout"))))
;; timeout
```

- `((v (recv ch)) corpo...)` é um ramo de recepção. `v` recebe um `Option<T>`.
- `((send ch x) corpo...)` é um ramo de envio.
- Quando vários ramos podem avançar ao mesmo tempo, um deles é escolhido ao acaso.
- Com `(else corpo...)` no final, `else` é executado quando nenhum ramo pode avançar imediatamente, e `select`
  não espera.

```lisp
(let ((ch (the Chan<int> (Chan::new 0))))
  (select
    ((v (recv ch)) (println "~a" v))
    (else (println "nothing ready"))))
;; nothing ready
```

## 4. Proteger dados compartilhados

Quando várias tarefas modificam o mesmo valor, proteja-o com `Mutex<T>`. `with-lock` pega a trava, vincula o
conteúdo a uma variável, executa o corpo e sempre libera a trava, qualquer que seja a forma de sair do corpo.
Atribuir à variável com `setf` dentro do corpo muda o conteúdo do `Mutex`.

`WaitGroup` é uma ferramenta para esperar até que um dado número de tarefas termine. Aumente a contagem com
`add`, faça cada tarefa chamar `done` ao terminar e faça `wait` até a contagem chegar a 0.

```lisp
(defun add-many ((counter Mutex<int>) (n int)) ()
  (dotimes (i n)
    (with-lock (c counter)
      (setf c (+ c 1)))))

(let ((counter (the Mutex<int> (Mutex::make 0)))
      (wg (the WaitGroup (WaitGroup::make))))
  (dotimes (i 4)
    (add wg 1)
    (task ((lambda () ()
             (add-many counter 1000)
             (done wg)))))
  (wait wg)
  (println "count = ~a" (with-lock (c counter) c)))    ; count = 4000
```

Se várias tarefas modificam o mesmo valor ao mesmo tempo sem passar por um `Mutex` ou um canal, o resultado
não é garantido. Passe os dados entre tarefas por canais sempre que puder, e compartilhe dados só quando
precisar.

## 5. Onde as tarefas alternam

As tarefas se alternam de forma cooperativa. Uma tarefa cede a vez a outras somente nestes pontos:

- `(yield)`, `(sleep segundos)`, `(wait handle)`
- Uma operação de canal que precisa esperar (`send`, `recv`, `select`)
- Uma operação de socket que precisa esperar (conectar, ler, escrever etc.)

O argumento de `sleep` é um número `f64` de segundos. Escreva `(sleep 1.0)`, não `(sleep 1)`.

As tarefas são executadas ao mesmo tempo em várias threads de SO. No entanto, quando o `typl` executa um
programa diretamente, só as tarefas que executam funções [compiladas](../guide/compile.md) vão para outras
threads. As demais tarefas são executadas em uma única thread, alternando nos pontos acima.

## 6. `thread`: executar em uma thread de SO dedicada

Um trabalho que não deve segurar outras tarefas, como chamar uma função C lenta ([FFI de C](../guide/ffi.md)),
é iniciado com `thread`. Escreve-se do mesmo jeito que `task`, e ele ganha uma thread de SO própria. Espere
que termine com `join`.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(let ((th (thread (fib 25))))
  (println "fib(25) = ~a" (join th)))     ; fib(25) = 75025
```

- O handle de `thread` tem o tipo `Thread<T>`. Como `wait`, `join` pode ser chamado quantas vezes quiser.
- `thread` só pode executar funções que possam ser compiladas. Ao executar no `typl`, a função que ele chama é
  compilada na hora antes de ser executada.

## 7. As tarefas e outros recursos

- Um `panic` dentro de uma tarefa para o programa inteiro.
- `throw` não alcança fora de uma tarefa. Um `throw` que sairia do corpo da tarefa vira um `panic`.
- A saída de um `println` nunca se mistura no meio da linha com a saída de outras tarefas.

## 8. O que ler em seguida

- [Tarefas e canais](../reference/functions/concurrency.md): a lista de funções
- [Referência de sintaxe, capítulo 12](../reference/syntax.md#12-concorrência-tarefas): onde as tarefas
  alternam em detalhes, e diferenças em relação ao Go
- [E/S de arquivos, streams e rede](../guide/io.md): escrever um servidor com tarefas
