# Campaign progress: локальные findings

Проверено 11 июля 2026 года в установленном StarCraft II на macOS. Файлы только читались; изменений в профиле игры не вносилось.

## Где лежит прогресс

Для профиля `2-S2-1-2048549` обнаружены два разных уровня данных:

1. `CampaignProgress.xml` — короткий список глобальных флагов кампаний. Он не содержит текущую миссию.
2. `Banks/*Campaign.SC2Bank` — XML-файлы с детальным состоянием: `LastMission`, `LastMap`, флаг успеха и `MissionCompletedCount`.

Следовательно, для определения «куда продолжать» нужно читать campaign bank, а не только `CampaignProgress.xml` или имя `.SC2Save`.

Корневой путь профиля:

```text
/Users/gamer/Library/Application Support/Blizzard/StarCraft II/Accounts/112261788/2-S2-1-2048549
```

## Актуальное состояние

Самое свежее сохранение относится к **Heart of the Swarm**:

- `Saves/SwarmCampaignSave.SC2Save` обновлён 10 июля 2026 в 17:53;
- `Saves/Campaign/Quick Save.SC2Save` обновлён в 17:50;
- `Banks/ZCampaign.SC2Bank` обновлён в 17:45.

Значения из `ZCampaign.SC2Bank`:

```text
LastMission            = ZLab1
LastSuccessfulMission  = ZLab1
LastMap                = ZStoryLab
LastMapSuccess         = 1
LastMissionSuccess     = 1
MissionCompletedCount  = 1
Difficulty             = 4   # внутреннее значение игры
```

Итог: в HotS пройдена одна миссия; продолжение находится в лабораторном/сюжетном хабе после неё.

## Другие найденные campaign banks

| Кампания | Bank | Найденный статус |
| --- | --- | --- |
| Wings of Liberty | `WCampaign.SC2Bank` | `LastMission = THanson01`, пройдено 4 миссии (`MissionCompletedCount = 4`) |
| Heart of the Swarm | `ZCampaign.SC2Bank` | `LastMission = ZLab1`, пройдена 1 миссия |
| Legacy of the Void | `PCampaign.SC2Bank` | `LastMission = PAiur01`, последняя миссия успешна |
| Nova Covert Ops | `NCampaign.SC2Bank` | `LastMission = Nova05`, пройдено 5 миссий |

`CampaignProgress.xml` при этом содержит лишь следующее:

```xml
<CampaignProgress id="HeartOfTheSwarm" tutorialfinished="0" campaignfinished="0"/>
<CampaignProgress id="LegacyOfTheVoidPrologue" tutorialfinished="0" campaignfinished="1"/>
<CampaignProgress id="LegacyOfTheVoid" tutorialfinished="0" campaignfinished="0"/>
```

То есть флаги главного интерфейса и реальное положение в кампании — разные данные. Например, в XML нет записи о Wings of Liberty и Nova, хотя их campaign bank-файлы существуют и содержат прогресс.

## Вывод для подменяемых модов

Если несколько модов используют один и тот же campaign bank (например, `ZCampaign.SC2Bank`) и один `campaignId`, они будут делить прогресс. Один лишь разбор `.SC2Save` не решает коллизию интерфейса SC2.

Для каждого мода нужны собственные, стабильные идентификаторы и изолированное хранилище:

- `modId` и версия/хеш мода;
- свой набор campaign bank-файлов;
- свой campaign save и ручные `.SC2Save`;
- небольшой manifest с `modId`, версией, `LastMission` и датой сохранения.

Лаунчер должен показывать прогресс из manifest/bank выбранного мода, а не доверять экрану «Кампания» в SC2. Перед копированием или переключением bank/save-файлов StarCraft II нужно полностью закрывать: во время проверки часть файлов изменялась/пересоздавалась работающей игрой или синхронизацией.

Также в `Accounts/112261788` есть отдельный профиль `2-S2-2-8582`. При реализации переключателя следует учитывать полный путь профиля, включая сегмент `2-S2-<region>-<profile>`.
