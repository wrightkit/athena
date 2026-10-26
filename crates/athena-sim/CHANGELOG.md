# Changelog

## [0.6.0](https://github.com/wrightkit/athena/compare/v0.5.0...v0.6.0) (2026-10-26)


### Features

* **sim:** model All Heroes from the catalog's Hero members ([#70](https://github.com/wrightkit/athena/issues/70)) ([9551dde](https://github.com/wrightkit/athena/commit/9551dde29d131d789df947475d49ae259e279e61)), closes [#69](https://github.com/wrightkit/athena/issues/69)
* **sim:** model Current Map from a scenario map input ([#73](https://github.com/wrightkit/athena/issues/73)) ([d013765](https://github.com/wrightkit/athena/commit/d0137651183a464dc5814a1c8939365ece9c5443)), closes [#71](https://github.com/wrightkit/athena/issues/71)
* **sim:** model vector values and player position/facing ([#76](https://github.com/wrightkit/athena/issues/76)) ([96d0547](https://github.com/wrightkit/athena/commit/96d05473685ab0d6f63ba60a923646c9c14250b1)), closes [#74](https://github.com/wrightkit/athena/issues/74)

## [0.5.0](https://github.com/wrightkit/athena/compare/v0.4.0...v0.5.0) (2026-10-26)


### Features

* **sim:** evaluate Workshop Setting values to their defaults ([#55](https://github.com/wrightkit/athena/issues/55)) ([cd3208a](https://github.com/wrightkit/athena/commit/cd3208ae014ca19b7d43a1c202ed219fff056ecc)), closes [#54](https://github.com/wrightkit/athena/issues/54)
* **sim:** instantiate Each Player rules per matching player ([#49](https://github.com/wrightkit/athena/issues/49)) ([415897e](https://github.com/wrightkit/athena/commit/415897e21165a23e6b0269359704dad8cfaa2ea1)), closes [#46](https://github.com/wrightkit/athena/issues/46)
* **sim:** model control-flow actions (skip, abort, loop, break/continue) ([#59](https://github.com/wrightkit/athena/issues/59)) ([1802717](https://github.com/wrightkit/athena/commit/1802717d4b890d8cee9a381fa11581eff6f0025b))
* **sim:** model element-context array values and Evaluate Once ([#62](https://github.com/wrightkit/athena/issues/62)) ([31a42d7](https://github.com/wrightkit/athena/commit/31a42d7d0ce7c9510c570522de80b1d1ca847aaf)), closes [#60](https://github.com/wrightkit/athena/issues/60)
* **sim:** model For Global/Player Variable and pair their End lines ([#52](https://github.com/wrightkit/athena/issues/52)) ([3c60fd6](https://github.com/wrightkit/athena/commit/3c60fd6508ae608a6c0b02a0ce7faff8d0b93346)), closes [#50](https://github.com/wrightkit/athena/issues/50)
* **sim:** model general and array values plus at-index writes ([#61](https://github.com/wrightkit/athena/issues/61)) ([2e950a1](https://github.com/wrightkit/athena/commit/2e950a1a6f4c6e65f4f31d2cd372aad7ff33a6e0))
* **sim:** model Random values from a scenario-supplied seed ([#66](https://github.com/wrightkit/athena/issues/66)) ([446f42b](https://github.com/wrightkit/athena/commit/446f42bb00fce4360ea6bbf665a1b9dd74d9cd6d)), closes [#65](https://github.com/wrightkit/athena/issues/65)
* **sim:** model statuses, buttons, start rule and effects ([#63](https://github.com/wrightkit/athena/issues/63)) ([b996c07](https://github.com/wrightkit/athena/commit/b996c07bc609e1a50e4650a90043cb3b1bc49489)), closes [#58](https://github.com/wrightkit/athena/issues/58)
* **sim:** model string values used by Bastion ([#64](https://github.com/wrightkit/athena/issues/64)) ([fe57ffe](https://github.com/wrightkit/athena/commit/fe57ffe22f807b9e513ce088e0b88f444bd05abd)), closes [#57](https://github.com/wrightkit/athena/issues/57)
* **sim:** record a per-tick timeline in the trace ([#53](https://github.com/wrightkit/athena/issues/53)) ([f163fdb](https://github.com/wrightkit/athena/commit/f163fdb58d0bf5e0fdc10abc4e5c3f9b8b165106)), closes [#48](https://github.com/wrightkit/athena/issues/48)

## [0.4.0](https://github.com/wrightkit/athena/compare/v0.3.0...v0.4.0) (2026-10-25)


### Features

* **sim:** simulate Wait Until, Wait Behavior and spawn state (layer 3) ([1d06477](https://github.com/wrightkit/athena/commit/1d06477a92e83de0a415eca54644bbc1419e0bfc)), closes [#42](https://github.com/wrightkit/athena/issues/42)


### Bug Fixes

* **probes:** make P7 importable and wait for spawn ([1263c9c](https://github.com/wrightkit/athena/commit/1263c9c5286ba40f08ced2a46cbedec4f44b90b7)), closes [#30](https://github.com/wrightkit/athena/issues/30) [#42](https://github.com/wrightkit/athena/issues/42)

## [0.3.0](https://github.com/wrightkit/athena/compare/v0.2.0...v0.3.0) (2026-10-25)


### Features

* **capture:** watch Inspector logs, dedup by source, complete ingest coverage ([#34](https://github.com/wrightkit/athena/issues/34)) ([40d450d](https://github.com/wrightkit/athena/commit/40d450d3d085e9a5574d8cff5bfb2b0eb6035e68))
* **sim:** apply hero settings to the player state table ([981f006](https://github.com/wrightkit/athena/commit/981f006eae1e11f5fde376c0e9fa7213b12b5e05)), closes [#29](https://github.com/wrightkit/athena/issues/29)
* **sim:** probe p7 hero settings and read reported player state ([#33](https://github.com/wrightkit/athena/issues/33)) ([3142b65](https://github.com/wrightkit/athena/commit/3142b65e85c822e3c70f536c4bc572ee4352db56))

## [0.2.0](https://github.com/wrightkit/athena/compare/v0.1.0...v0.2.0) (2026-10-21)


### Features

* scenario format — players, injected events and expectations ([#18](https://github.com/wrightkit/athena/issues/18)) ([238db2c](https://github.com/wrightkit/athena/commit/238db2c833afc52580595c6a169be28706b42e87)), closes [#8](https://github.com/wrightkit/athena/issues/8) [#8](https://github.com/wrightkit/athena/issues/8)
* **sim:** model HUD elements in the trace ([8184b5a](https://github.com/wrightkit/athena/commit/8184b5aefd92ead8c6e221a8ce85e884d9fd887f)), closes [#10](https://github.com/wrightkit/athena/issues/10)
* **sim:** player state table with Damage and Heal ([b88b328](https://github.com/wrightkit/athena/commit/b88b3289879f7fd200d90a58938dc0942154d487)), closes [#9](https://github.com/wrightkit/athena/issues/9)
* **sim:** report unmodeled behavior in a serializable trace ([#14](https://github.com/wrightkit/athena/issues/14)) ([4f90bba](https://github.com/wrightkit/athena/commit/4f90bba955a251d3b0c8f06f5f346a01a71e8c68))
