# SIGNAL v0.1 · BTC/USDT 시장 분석 터미널

문서 기준일: 2026-10-10. 저장소: https://github.com/Furu002/Bitcoin-Anlaysis-v0.1

현재 개발 기준은 Windows/macOS 데스크톱에서 동작하는 BTC/USDT 현물 분석 터미널이다. 합성 시뮬레이션으로 시작하고 사용자가 선택하면 Binance 공개 체결·호가 데이터를 연결한다. Tauri 2, React, TypeScript, Rust를 사용한다. 이 문서는 이전 국내주식 스캘핑 서비스 명세를 현재 사용자 결정에 맞춰 전체 개정한 것이다.

## 1. 제품 목표와 확정 범위

같은 화면에서 체결 흐름, 유효한 호가, 관측 CVD, 호가 불균형과 데이터 상태를 확인하고, 로컬에 기록한 입력을 같은 엔진으로 재생한다.

- 시장: Binance BTC/USDT 현물 한 종목. 전체 Bitcoin 시장을 대표한다고 주장하지 않는다.
- 플랫폼: Tauri 2 Windows/macOS 데스크톱. 개발 확인용 브라우저 화면도 같은 Rust 엔진을 사용한다.
- 입력: 원시 trade 스트림, diff depth 100ms 스트림, REST 호가 스냅샷.
- 분석: 체결 테이프, 관측 가격 차트, 세션 CVD, 스프레드, 상위 20단계 불균형.
- 보관: 사용자 PC의 SQLite 녹화·재생. 클라우드 업로드와 텔레메트리는 구현하지 않는다.
- 현재 제품에는 주문 실행, 자동매매, 주문 버튼, 계좌 조회, API 키 입력, 증권계좌 연동이 없다.
- 로그인·결제·구독·다종목 스캐너·KRX/NXT·뉴스·AI 조건 작성은 현재 v0.1 범위에 포함하지 않는다.

## 2. 우선순위와 완료 기준

첫 우선순위는 인터넷 연결 없이 실행되는 시뮬레이션이다. 다음은 호가 동기화의 정확성, 누락 상태의 가시성, 녹화·재생의 재현성이다. 기능 수보다 데이터가 유효한지 판단할 수 있어야 한다.

첫 수직 구현의 완료 기준은 시뮬레이션 입력 → Rust 계산 → 화면 → SQLite 기록 → 같은 Rust 계산으로 재생이 연결되고, 호가 누락 시 불균형이 표시되지 않는 것이다. 거래소 실시간 연결의 성공 여부, OS별 배포 검증 및 상용 데이터 이용권 확보는 각각 별도로 기록한다.

## 3. 현재 기능

- 기본 시작 모드: Simulation. 고정 seed의 합성 BTC/USDT 호가·체결을 생성하며 화면에 SYNTHETIC DATA를 표시한다.
- 모드 전환: Simulation / Connect live / 녹화 세션 Replay. 실시간 연결은 사용자의 명시적 선택으로 시작한다.
- 호가 패널: 매수·매도 각 10단계 표시, 수량과 누적 수량, 스프레드와 업데이트 ID 표시. 불균형은 각 20단계를 사용한다.
- 체결 테이프: 최근 100개 관측 원시 체결. UTC 시각, 가격, BTC 수량과 B/S 방향을 표시한다.
- 차트: 최근 최대 120초의 관측 체결 가격과 세션 CVD. 미래 단계의 OHLCV 봉차트나 footprint로 표시하지 않는다.
- 상태 패널: 호가·체결 수신 나이, 동기화 상태, 시계 오프셋, 누락·중복 진단, 녹화 상태.
- 녹화: Record session을 누르면 새로운 분석 세션과 데이터 epoch를 시작해 스냅샷 초기화가 포함되도록 한다.
- 재생: 저장 세션 선택, 일시정지, 1/2/4/8배속. 종료되면 가상 시각을 마지막 입력 시각에 고정한다.
- 장애 확인: Simulation에서 Inject depth gap으로 호가 순서 누락을 재현한다. 호가·불균형이 숨겨지고 새 스냅샷으로 회복한다.
- 설명 화면: CVD 방향 의미, 불균형 공식, 신선도 임계값, 데이터 출처와 DB 위치.

## 4. 데이터 출처와 접근 정책

공식 문서의 공개 시장 데이터 전용 주소만 사용한다.

```text
wss://data-stream.binance.vision/stream?streams=btcusdt@trade/btcusdt@depth@100ms
https://data-api.binance.vision/api/v3/depth?symbol=BTCUSDT&limit=1000
```

공개 전용 주소는 Binance 문서상 인증이 필요하지 않다. 계정이나 API 키를 받지 않으며 인증·주문용 주소를 호출하지 않는다. TLS 인증서 검증은 유지한다. 시뮬레이션은 거래소 네트워크 요청을 만들지 않는다.

403/451/418은 실시간 소스 접근을 중단한다. 429는 Retry-After만큼 대기하며 헤더가 없으면 최소 60초를 적용한다. 같은 앱 실행 중 모드 전환으로 이 cooldown을 무력화하지 않는다. 일반 네트워크 오류는 제한된 지수 backoff와 jitter를 사용한다. 지역 제한 우회, 프록시 회전, 도메인 변경, 타인 프로젝트의 코드 복제나 무단 scraping은 구현하지 않는다.

## 5. 호가 동기화 계약

1. WebSocket을 먼저 연다.
2. REST 요청을 기다리는 동안 diff depth 이벤트를 계속 수신하고 버퍼에 넣는다.
3. REST 스냅샷의 lastUpdateId를 기준으로 호가를 초기화한다.
4. u가 스냅샷 ID 이하인 이전 이벤트는 버린다.
5. 첫 유효 이벤트는 U ≤ lastUpdateId + 1 ≤ u를 만족해야 한다.
6. 각 가격의 수량은 절대값으로 교체한다. 수량 0은 삭제한다.
7. 다음 이벤트도 현재 ID + 1을 포함해야 한다. 이전/중복 이벤트는 신선도를 갱신하지 않는다.
8. 앞으로 건너뛴 ID, 잘못된 데이터, 교차·잠긴 호가, 깊이 부족이나 메모리 한도 초과는 호가를 무효화하고 새 연결·스냅샷으로 복구한다.

스냅샷만 받은 상태는 synchronized로 표시하지 않는다. Spot에는 futures의 pu 규칙을 적용하지 않는다. snapshot보다 오래된 최초 delta가 버퍼에 들어 있어도 최신 스냅샷 응답 시각을 delta 수신시각처럼 사용하지 않는다.

스냅샷은 각 1,000단계로 제한된다. 처음 관측한 매수 최하단·매도 최상단 경계를 추적해 그 밖의 새 가격이 상위 20단계의 완전성을 보장하는 것처럼 보이지 않게 한다. coverage가 부족하면 수치 공개를 중지하고 재동기화한다.

## 6. 체결 방향과 CVD 정의

Binance의 m은 buyer is maker 여부다.

- m=false: 매수자가 유동성을 소모한 buy aggressor. CVD에 +BTC 수량.
- m=true: 매수자가 maker이므로 매도자가 sell aggressor. CVD에 −BTC 수량.

CVD는 세션 시작 이후 관측한 매수 주도 BTC 수량 합에서 매도 주도 BTC 수량 합을 뺀 값이다. 가격이 올랐다는 이유로 매수 방향을 추정하지 않는다. 모든 체결에는 매수자와 매도자가 있으므로 B/S는 단순 매수·매도 참여자 수가 아니라 aggressor 의미다.

Trade ID로 중복·이전 체결을 제거한다. ID가 앞으로 건너뛰거나 연결 epoch가 바뀌면 해당 세션 CVD를 incomplete로 유지한다. 복구 후 정상 체결이 들어와도 이전 누락이 자동으로 사라진 것으로 표시하지 않는다. 새로운 모드 또는 녹화 세션 시작은 새 기준점을 만든다. 값은 일일 전체 시장 CVD가 아니라 관측 세션 값이다.

## 7. 불균형·스프레드·가격

불균형 = (상위 20단계 매수 BTC 합 − 상위 20단계 매도 BTC 합) / 양쪽 BTC 합. 분모가 0이거나 호가가 미동기화·오래됨·깊이 부족 상태이면 N/A다. 양수는 관측 잔량의 매수 편중이며 상승 확률이나 투자 성과로 해석하지 않는다.

스프레드는 같은 유효 호가의 최우선 ask − 최우선 bid다. 마지막 가격은 마지막 관측 체결가이며 trade freshness를 함께 표시한다. 오래된 마지막 가격을 새로운 체결처럼 재발행하지 않는다.

가격·수량은 소수 8자리의 정확한 정수로 처리한다. wire와 DB는 소수 문자열을 보존한다. 음수, 지수 표기, 과도한 소수 자릿수, overflow, 잘못된 형식은 거부한다. CVD 누적 합은 더 큰 정수 폭으로 처리하고 UI 차트·비율 표시 단계에서만 floating point로 변환한다.

## 8. 피드 상태와 시간

Depth와 trade의 신선도를 별도로 판단한다. 정상 메시지가 들어왔다는 이유만으로 모든 패널을 healthy로 만들지 않는다.

- Book stale: 마지막 적용 가능한 depth 수신 후 2초 초과.
- Trade stale: 마지막 유효·중복 아닌 체결 수신 후 5초 초과.
- Transport stall: 15초 동안 WebSocket 프레임이 없으면 재연결.
- Connection rotation: 24시간 만료 전에 재연결. serverShutdown도 복구 경계로 처리.
- Ping/pong: 동일 payload pong을 신속하게 flush하며 write timeout을 제한.
- 표시 상태: healthy / syncing / degraded / blocked / cooldown. paused는 Simulation/Replay의 가상 시계 정지를 나타낸다.

수신시각은 단조 시계에 기준 Unix 시각을 연결해 만든다. 네트워크 어댑터에서 엔진 큐에 넣기 전에 시각을 찍으므로 큐 지연이 오래된 입력을 최신으로 보이게 만들지 않는다. 거래소 발생시각과 수신시각의 차이는 시계 오차를 포함한 offset으로 표시하며 one-way latency 측정값이라고 부르지 않는다. 화면 시각은 UTC로 명시한다.

## 9. 시스템 구조

```text
Simulation / Binance public adapter / SQLite replay
                         ↓
         ordered normalized input envelopes
                         ↓
          one Rust engine + local recorder
                         ↓
     bounded view over Tauri IPC, approximately 10 Hz
                         ↓
             React / TypeScript terminal
```

브라우저 개발 미리보기는 loopback에만 열리는 Vite bridge가 동일한 Rust CLI를 실행한다. 별도의 TypeScript 계산 엔진을 만들지 않는다. 배포형 Tauri는 로컬 IPC를 사용하며 개발용 웹 서버를 시작하지 않는다.

Rust actor가 호가, 체결, 지표, source lifecycle과 SQLite를 소유한다. React는 bounded view를 받아 화면을 표시한다. 모드 전환·재동기화마다 generation이 바뀌고 이전 연결의 늦은 메시지는 무시한다.

## 10. 로컬 저장 모델과 재생

SQLite는 WAL과 명시적 transaction을 사용한다. sessions에는 ID, source, 시작시각, closed 상태를 저장한다. events에는 session ID, 순번, 수신시각, format version과 정규화 payload를 저장한다. 이벤트 종류는 reset, status, snapshot, depth, trade다.

기록은 128개 입력 또는 약 300ms마다 flush하고 정상 종료 시 최종 flush한다. 저장 실패는 화면 오류로 표시하고 녹화를 중단한다. 갑작스러운 프로세스 종료에서는 미커밋 batch가 손실될 수 있다. 전원 차단 durability, 저장 quota·retention 및 상세 crash recovery UI는 후속 검증 항목이다.

재생은 DB 순번대로 읽어 같은 Engine.apply 경로를 거친다. 기록 형식 버전, 연속 순번, payload와 DB 수신시각 일치 및 시간 순서를 검증한다. wall-clock 대신 기록의 가상 수신시각을 사용한다. v0.1은 한 번의 재생을 100,000개 이벤트 및 encoded payload 64 MiB로 제한한다. 더 큰 세션에는 오류를 표시하며 pagination/seek/checkpoint는 다음 단계다.

Tauri DB는 플랫폼별 local app data에 저장한다. 개발 미리보기 DB는 프로젝트의 .signal/preview.sqlite3이다. 녹화·DB·컴파일러·node_modules·build output은 Git에 올리지 않는다.

## 11. 화면과 사용 순서

상단에는 제품명·버전·ANALYSIS ONLY·UTC 시계, BTC/USDT 현물 종목과 데이터 모드를 표시한다. 데이터 출처 배너 아래에 마지막 관측가, 세션 CVD, 상위 20단계 불균형, 스프레드와 체결 수를 배치한다. 본문은 가격/CVD 차트, 호가와 체결 테이프다. 하단은 Feed health와 Recordings다.

1. Simulation에서 가격과 체결을 확인한다.
2. Inject depth gap을 눌러 값이 무효화되고 복구되는지 확인한다.
3. Record session을 시작하고 몇 초 후 Stop recording한다.
4. Recordings에서 세션을 선택해 재생하고 배속·일시정지를 사용한다.
5. 필요할 때 Connect live를 선택한다. 차단·cooldown은 그대로 표시한다.

방향은 색뿐 아니라 B/S와 부호로 표시한다. 비동기 명령 중에는 반복 동작을 제한한다. 실시간 입력은 일시정지하지 않으며 Simulation 또는 Replay로 전환해 관찰을 멈출 수 있다.

## 12. 테스트와 실제 검증 상태

Rust 테스트는 정확한 소수 처리, buffer/snapshot bridge, overlap·중복·zero 삭제, forward gap, crossed book, coverage, aggressor 부호, 체결 중복과 CVD completeness, 독립 수신 나이와 SQLite 재생 일치를 다룬다.

통합 smoke는 Simulation → Record → Stop → Replay 후 exact CVD, 매수·매도 수량, 체결 수, book ID, 호가 전체와 tape가 일치하는지 비교한다. 이후 depth gap을 주입해 값 무효화·복구·incomplete 표시를 확인한다.

현재 관측: 프런트엔드 build, Rust core 15개 테스트와 최종 receive-clock 변경 이후 offline smoke가 통과했다. 브라우저에서 일시정지, 녹화, 세션 목록, 8배속 재생과 완료 event count를 확인했다. Windows GNU 개발 빌드가 컴파일·실행됐고 native app-data DB 경로로 Tauri IPC 사용을 확인했다. 실제 Binance 공개 피드에서 REST 스냅샷과 WebSocket depth 동기화, healthy 상태, 체결·CVD·불균형 갱신을 확인했다. 이는 짧은 연결 검사이며 24시간 soak나 모든 지역의 접근 가능성을 보장하지 않는다. 상세 결과는 VALIDATION.md에 기록한다.

GitHub Actions에는 Windows/macOS build·test·smoke·desktop check를 구성했다. 설정이 존재한다는 사실은 각 OS에서 통과했다는 의미가 아니다. macOS 현지 실행, MSVC release, 두 macOS architecture, signing/notarization 및 장시간 부하 검증은 후속 과제다.

## 13. 개발 단계

- Phase 1: 이번 수직 구현. Offline simulation, 공개 adapter, 정확한 동기화, 지표와 health, SQLite 녹화·재생, 테스트와 실행 확인.
- Phase 2: 장시간 soak, network fault proxy, 실제 source fixtures, queue·disk stall·storage failure, retention/quota, crash recovery, replay pagination/seek/checkpoint, 측정된 ingest/render 지연.
- Phase 3: 시간 bucket CVD, 체결 수량 필터, footprint, depth history, 사용자 layout과 키보드 접근성. 각 지표의 source·unit·window·completeness 계약 유지.
- Phase 4: Windows MSVC와 macOS 양 architecture CI, 서명·notarization·설치 검증, dependency license 및 실제 상용 데이터 계약 검토.

각 단계는 완료 조건으로 판단하며 개발 기간이나 수익률을 약속하지 않는다. 주문 실행은 후속 단계에 자동 포함되지 않는다.

## 14. 데이터 이용권과 법률 제약

공개 주소에 기술적으로 접속할 수 있다는 사실만으로 저장·재배포·판매·상용 표시 권한을 확보했다고 판단하지 않는다. 외부 공개나 유료 서비스 전에 최신 Binance 및 지역·법인별 약관, 시장 데이터 사용·보관 조건과 해당 관할권의 서비스 형태를 검토해야 한다.

구현 시 확인한 terms 페이지에서는 연구 도구가 실질적인 지역 조항을 읽지 못했다. 따라서 허용된다고 주장하지 않는다. 이 PC의 locale로 관할권을 추정하지 않는다. 기록 기능은 로컬 엔지니어링 기능이며 출시 전 허용 범위를 별도로 확인한다.

SIGNAL은 관측 사실을 설명한다. 투자 수익, 가격 방향, 개인별 매수·매도 지시, 주문 실행이나 거래 결과를 보장하지 않는다. 데이터가 유효하지 않으면 이를 사용하는 유동성 지표를 공개하지 않는다.

## 15. 운영 한도와 남은 위험

- 단일 venue·symbol이며 전체 Bitcoin 시장 coverage가 아니다.
- 최근 tape 100개와 chart sample 1,200개만 화면에 유지한다. 화면 제한은 원천 누락과 별개의 제한이다.
- ingest 큐 8,192개, bootstrap delta 4,096개, 호가 전체 20,000레벨 한도. overflow·coverage 문제는 resync로 처리한다.
- 재생 100,000개·64 MiB 한도, 현재 seek와 대용량 pagination 없음.
- SQLite 동기 transaction은 bounded batch지만 장시간 disk stall 성능은 아직 측정하지 않았다.
- snapshot이 처음보다 뒤로 밀리면 보수적으로 새 epoch를 시작한다. 처음부터 모든 연결 공백을 복구하는 과거 체결 backfill은 없다.
- 실제 source 접근은 지역·네트워크·정책에 따라 달라진다. offline 성공을 live 연결 성공으로 해석하지 않는다.
- 설치 패키지와 자동 업데이트는 현재 release-ready 상태가 아니다.

## 16. 코드와 공식 자료

코드 경로는 crates/signal-core, src, src-tauri, scripts다. PLAN.md는 단계별 계약, DATA_SOURCES.md는 출처와 release gate, VALIDATION.md는 실제 검사 결과를 담는다.

- [GitHub 저장소](https://github.com/Furu002/Bitcoin-Anlaysis-v0.1)
- [Binance 공개 시장 데이터 전용 주소](https://github.com/binance/binance-spot-api-docs/blob/master/faqs/market_data_only.md)
- [Binance Spot WebSocket 규격](https://github.com/binance/binance-spot-api-docs/blob/master/web-socket-streams.md)
- [Binance Spot REST 규격](https://github.com/binance/binance-spot-api-docs/blob/master/rest-api.md)
- [Binance Terms](https://www.binance.com/en/terms)
- [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri frontend → Rust commands](https://v2.tauri.app/develop/calling-rust/)

현재 개발의 기준은 시뮬레이션에서 재현 가능한 분석 흐름, 호가 동기화, 데이터 상태 가시성과 녹화·재생 정확성이다. 확장 기능과 실제 데이터의 상용 제공은 각각의 검증·이용권 조건을 통과한 뒤 진행한다.
