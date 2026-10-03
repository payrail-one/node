# Исторический performance baseline Temtum

## Подтверждённый результат

В соседнем репозитории найден отчёт:

`temtum-site/src/reports/CSIRUKPRJ-594-Performance-Test-Temtum-Crypto-Currency-Network-v1.0.pdf`

BSI проводила испытания с 31 мая по 1 июня 2019 года; definitive report выпущен
10 июня 2019 года. Использовался предоставленный командой test harness и один
сервер Ubuntu 18.04.2 с Intel Xeon E5-1650 v4, 6 cores и 64 GB RAM.

| Продолжительность | Прогоны | Средний TPS | Минимум | Максимум |
| ---: | ---: | ---: | ---: | ---: |
| 30 секунд | 3 | 1493 | 1449 | 1530 |
| 60 секунд | 3 | 1053 | 937 | 1264 |
| 10 минут | 3 | 1253 | 1095 | 1385 |
| 30 минут | 3 | 1531 | 1208 | 2068 |

Среднее по двенадцати прогонам — **1332 TPS**. Это исторический независимо
зафиксированный результат, который новая система не должна потерять как
минимальный ориентир производительности на сопоставимом железе.

## Чего отчёт не доказывает

Документ не фиксирует число валидаторов, географию, сетевую задержку, тип и
размер транзакции, момент finality, rejected/duplicate operations, состояние
индексатора, Byzantine/failure сценарии или скорость initial state sync.
Следовательно, 1332 TPS нельзя автоматически называть finalized multi-node TPS
и нельзя напрямую сравнивать с карточным message processing.

## Найденные механизмы старого кода

- LMDB и cursor-based чтение цепочки;
- отдельное сжатое хранение transaction payload блока;
- WebSocket и HTTP catch-up;
- последовательная синхронизация блоков chunks по 5 блоков;
- NATS для распространения новых блоков;
- проверка расхождения высоты и выбор peer с максимальной высотой.

Сохраняем как проверяемые гипотезы компактное хранение, compression, chunked
transfer и отделение live propagation от catch-up. Не переносим центральный
master/leader, JSON как основной binary protocol, shared-secret peer admission,
доверие одному источнику sync или случайную сверку блока вместо finality proof.

### Что переносится в новый контур

| Temtum | Новая реализация | Решение |
| --- | --- | --- |
| `BLOCK_ADDED` через NATS Streaming | отдельный live path для finalized blocks; bounded peer transport для pending transactions | сохранить разделение путей, заменить центральный broker на authenticated peer propagation |
| WebSocket `QUERY/RESPONSE_SYNC_BLOCKCHAIN` | finalized snapshot + параллельные content-addressed chunks + tail catch-up | сохранить request/response и retry, не доверять высоте или одному peer |
| `BLOCKS_PER_CHUNK = 5` | bounded chunks с явными byte limits, hash и manifest | сохранить chunking как принцип; размер выбирать benchmark, не старую константу |
| LMDB cursor reads | последовательный snapshot/export reader за storage trait | переиспользовать паттерн чтения, но не привязывать domain core к LMDB |
| отдельное compressed transaction payload блока | отдельный canonical payload и выбранный после benchmark compression layer | сохранить идею; формат и decompression limits определить заново |
| reconnect + resync после разрыва NATS | идемпотентные operation/chunk IDs и восстановление от finalized checkpoint | сохранить recovery-сценарий и превратить его в обязательный chaos test |
| полная выдача transaction pool | announce → request → bounded transaction payload | заменить, чтобы один peer не создавал memory/bandwidth amplification |

Исходные TypeScript-модули остаются ценными как набор требований, failure
сценариев и входов для сравнительных тестов. Их transport/authentication код не
подключается как зависимость к Rust node: это сохранило бы старые trust
assumptions внутри нового security boundary.

## Обязательный новый benchmark contract

Каждый результат привязывается к commit, конфигурации и аппаратной спецификации.
Минимальная матрица:

- 1-node execution baseline для прямого сравнения с историческим отчётом;
- 7 validators в 3 failure domains;
- p50/p95/p99 acknowledgement, inclusion и deterministic finality;
- steady load, burst, hotspot account и массовые повторы;
- отказ одного валидатора, затем двух; slow proposer и 100–300 ms RTT;
- catch-up работающей ноды одновременно с live traffic;
- clean-node bootstrap из проверенного finalized snapshot;
- snapshot download, hash/finality verification, state apply и tail catch-up как
  отдельные измеряемые фазы;
- bytes/s, blocks/s, state entries/s, CPU, RAM, disk IOPS и network transfer;
- нулевые потерянные accepted operations и RPO 0 для committed ledger.

Для state sync фиксируется не только полное время, но и размер состояния.
Catch-up обязан устойчиво обрабатывать историю быстрее live production; точный
коэффициент и production SLO утверждаются после первого Rust node harness.
