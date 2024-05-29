# Indexer Integration Tests Transaction Generator

* Indexer Integration Tests Transaction Generator is to generate transaction protos
  based on input Move files.


## Design 

```
Test Cases                                       Txn Protos          
                                                                     
┌─────────┐            ┌────────────────┐        ┌─────────┐         
│         │            │                │        │         │         
│   ┌─────┼───┐ ──────►│ Txn Generator  │ ─────► │   ┌─────┼───┐     
│   │     │   │        │ ┌────────────┐ │        │   │     │   │     
└───┼────┬┴───┼────┐   │ │ Fullnode   │ │        └───┼────┬┴───┼────┐
    │    │    │    │   │ │            │ │            │    │    │    │
    └────┼────┘    │   │ └────────────┘ │            └────┼────┘    │
         │         │   │                │                 │         │
         └─────────┘   └────────────────┘                 └─────────┘

```

## Config

* `test_cases_folder_path`: the path to the main folder of test cases.
* `test_case_config_name`: the config name for each test case, default to `config.yaml` 