# SC2 save / mod parity — findings

Дата анализа: 2026-07-11  
Источник: локальные `.SC2Save` в `~/Library/Application Support/Blizzard/StarCraft II/Accounts/112261788/`.

## Метод

Каждый `.SC2Save` — MPQ-архив. Внутри есть файл `save.details`, содержащий, среди прочего, явные строки зависимостей: пути к карте, кампаниям и `.SC2Mod`.

Для анализа использован скрипт рядом: `inspect_sc2_saves.py`. Он не изменяет сейвы.

## Результат

Проверено **58** файлов `.SC2Save` из двух локальных SC2-аккаунтов. В **27** из них указаны сторонние моды; остальные 31 не содержат в `save.details` пути к стороннему `.SC2Mod`.

Стандартные зависимости Blizzard — `Mods/Liberty.SC2Mod`, `Mods/Swarm.SC2Mod` и `Mods/Void.SC2Mod`; ниже перечислены только дополнительные модули.

| Сторонний мод, как записан в сейве | Кол-во сейвов | Сейвы / миссии |
| --- | ---: | --- |
| `Mods/VioletsHoTSReworkMod.SC2Mod` | 20 | `Fire in the Sky` (1); `The Crucible` (6, включая Quick Save и автосейвы); `Old Soldiers` (4 автосейва); `Baneling Evolution` (2); `Waking the Ancient` (7) |
| `Mods/LotV-Fight with ally!.SC2Mod` | 4 | `Forbidden Weapon` (1); `Salvation` (3, включая campaign save) |
| `Mods/ylvae_hots_allies.SC2Mod` | 2 | `Old Soldiers1.SC2Save`, `Old Soldiers2.SC2Save` |
| `Mods/HOTSRandomizer.SC2Mod` | 1 | `Phantoms of the Void.SC2Save` |

## Связка «сейв → мод → карта»

| Сейв/семейство | Сторонний мод | Карта из метаданных |
| --- | --- | --- |
| `Fire in the Sky.SC2Save` | `VioletsHoTSReworkMod` | `Campaign/Swarm/ZChar02.SC2Map` |
| `The Crucible` / Quick Save / связанные автосейвы | `VioletsHoTSReworkMod` | `Campaign/Swarm/ZZerus02.SC2Map` |
| автосейвы `Old Soldiers` | `VioletsHoTSReworkMod` | `Campaign/Swarm/ZChar03.SC2Map` |
| `Queen Evolution` / `Baneling Evolution` | `VioletsHoTSReworkMod` | `Campaign/Swarm/Evolution/ZEvolutionBaneling.SC2Map` |
| `Waking the Ancient` | `VioletsHoTSReworkMod` | `Campaign/Swarm/ZZerus01.SC2Map` |
| `Forbidden Weapon.SC2Save` | `LotV-Fight with ally!` | `Campaign/Void/PPurifier01.SC2Map` |
| `Salvation` | `LotV-Fight with ally!` | `Campaign/Void/PAiur06.SC2Map` |
| `Old Soldiers1/2.SC2Save` | `ylvae_hots_allies` | `Campaign/Swarm/ZChar03.SC2Map` |
| `Phantoms of the Void.SC2Save` | `HOTSRandomizer` | `Campaign/Swarm/ZHybrid03.SC2Map` |

## Важное для parity

- У `Old Soldiers` есть две разные семьи: ручные `Old Soldiers1/2` требуют `ylvae_hots_allies`, а автосейвы — `VioletsHoTSReworkMod`. Не считать их совместимыми без проверки.
- Сейв фиксирует имя/путь требуемого модуля. Это даёт точную стартовую точку для восстановления, но **этот проход не извлекал версию, publication ID или хэш содержимого мода**. Для полного binary parity нужен именно тот билд каждого `.SC2Mod`.
- Поиск в `~/Library/Application Support/Blizzard/StarCraft II` не нашёл файлов с этими четырьмя точными именами. Это не исключает наличие копий в каталоге установки игры или в Battle.net cache, но в локальной папке данных SC2 их нет.

## Как повторить анализ

```bash
python3 -m pip install mpyq
python3 inspect_sc2_saves.py \
  "$HOME/Library/Application Support/Blizzard/StarCraft II/Accounts/112261788"
```

Скрипт читает только `.SC2Save`, группирует их по набору зависимостей и печатает относительные пути файлов.
