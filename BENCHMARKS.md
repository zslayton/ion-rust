# Benchmarks

## Table of Contents

- [Benchmark Results](#benchmark-results)
    - [VarUInt Encoding: 1 byte(s)](#varuint-encoding:-1-byte(s))
    - [VarUInt Decoding: 1 byte(s)](#varuint-decoding:-1-byte(s))
    - [VarUInt Encoding: 2 byte(s)](#varuint-encoding:-2-byte(s))
    - [VarUInt Decoding: 2 byte(s)](#varuint-decoding:-2-byte(s))
    - [VarUInt Encoding: 3 byte(s)](#varuint-encoding:-3-byte(s))
    - [VarUInt Decoding: 3 byte(s)](#varuint-decoding:-3-byte(s))
    - [VarUInt Encoding: 4 byte(s)](#varuint-encoding:-4-byte(s))
    - [VarUInt Decoding: 4 byte(s)](#varuint-decoding:-4-byte(s))
    - [VarUInt Encoding: 5 byte(s)](#varuint-encoding:-5-byte(s))
    - [VarUInt Decoding: 5 byte(s)](#varuint-decoding:-5-byte(s))
    - [VarUInt Encoding: 6 byte(s)](#varuint-encoding:-6-byte(s))
    - [VarUInt Decoding: 6 byte(s)](#varuint-decoding:-6-byte(s))
    - [VarUInt Encoding: 7 byte(s)](#varuint-encoding:-7-byte(s))
    - [VarUInt Decoding: 7 byte(s)](#varuint-decoding:-7-byte(s))
    - [VarUInt Encoding: 8 byte(s)](#varuint-encoding:-8-byte(s))
    - [VarUInt Decoding: 8 byte(s)](#varuint-decoding:-8-byte(s))

## Benchmark Results

### VarUInt Encoding: 1 byte(s)

|         | `Ion v1.0`               | `Ion v1.1`                       |
|:--------|:-------------------------|:-------------------------------- |
| **`1`** | `88.38 us` (✅ **1.00x**) | `36.98 us` (🚀 **2.39x faster**)  |

### VarUInt Decoding: 1 byte(s)

|         | `Ion v1.0`               | `Ion v1.1`                       |
|:--------|:-------------------------|:-------------------------------- |
| **`1`** | `64.46 us` (✅ **1.00x**) | `37.93 us` (✅ **1.70x faster**)  |

### VarUInt Encoding: 2 byte(s)

|         | `Ion v1.0`               | `Ion v1.1`                       |
|:--------|:-------------------------|:-------------------------------- |
| **`2`** | `79.64 us` (✅ **1.00x**) | `66.12 us` (✅ **1.20x faster**)  |

### VarUInt Decoding: 2 byte(s)

|         | `Ion v1.0`               | `Ion v1.1`                       |
|:--------|:-------------------------|:-------------------------------- |
| **`2`** | `68.81 us` (✅ **1.00x**) | `37.88 us` (🚀 **1.82x faster**)  |

### VarUInt Encoding: 3 byte(s)

|         | `Ion v1.0`               | `Ion v1.1`                       |
|:--------|:-------------------------|:-------------------------------- |
| **`3`** | `71.23 us` (✅ **1.00x**) | `73.93 us` (✅ **1.04x slower**)  |

### VarUInt Decoding: 3 byte(s)

|         | `Ion v1.0`               | `Ion v1.1`                       |
|:--------|:-------------------------|:-------------------------------- |
| **`3`** | `77.97 us` (✅ **1.00x**) | `38.72 us` (🚀 **2.01x faster**)  |

### VarUInt Encoding: 4 byte(s)

|         | `Ion v1.0`               | `Ion v1.1`                       |
|:--------|:-------------------------|:-------------------------------- |
| **`4`** | `73.17 us` (✅ **1.00x**) | `66.39 us` (✅ **1.10x faster**)  |

### VarUInt Decoding: 4 byte(s)

|         | `Ion v1.0`               | `Ion v1.1`                       |
|:--------|:-------------------------|:-------------------------------- |
| **`4`** | `81.43 us` (✅ **1.00x**) | `37.82 us` (🚀 **2.15x faster**)  |

### VarUInt Encoding: 5 byte(s)

|         | `Ion v1.0`               | `Ion v1.1`                       |
|:--------|:-------------------------|:-------------------------------- |
| **`5`** | `73.04 us` (✅ **1.00x**) | `70.43 us` (✅ **1.04x faster**)  |

### VarUInt Decoding: 5 byte(s)

|         | `Ion v1.0`               | `Ion v1.1`                       |
|:--------|:-------------------------|:-------------------------------- |
| **`5`** | `91.78 us` (✅ **1.00x**) | `37.86 us` (🚀 **2.42x faster**)  |

### VarUInt Encoding: 6 byte(s)

|         | `Ion v1.0`               | `Ion v1.1`                       |
|:--------|:-------------------------|:-------------------------------- |
| **`6`** | `85.33 us` (✅ **1.00x**) | `70.40 us` (✅ **1.21x faster**)  |

### VarUInt Decoding: 6 byte(s)

|         | `Ion v1.0`                | `Ion v1.1`                       |
|:--------|:--------------------------|:-------------------------------- |
| **`6`** | `103.15 us` (✅ **1.00x**) | `37.79 us` (🚀 **2.73x faster**)  |

### VarUInt Encoding: 7 byte(s)

|         | `Ion v1.0`               | `Ion v1.1`                       |
|:--------|:-------------------------|:-------------------------------- |
| **`7`** | `74.07 us` (✅ **1.00x**) | `48.20 us` (✅ **1.54x faster**)  |

### VarUInt Decoding: 7 byte(s)

|         | `Ion v1.0`                | `Ion v1.1`                       |
|:--------|:--------------------------|:-------------------------------- |
| **`7`** | `114.42 us` (✅ **1.00x**) | `39.43 us` (🚀 **2.90x faster**)  |

### VarUInt Encoding: 8 byte(s)

|         | `Ion v1.0`                | `Ion v1.1`                       |
|:--------|:--------------------------|:-------------------------------- |
| **`8`** | `106.12 us` (✅ **1.00x**) | `59.96 us` (✅ **1.77x faster**)  |

### VarUInt Decoding: 8 byte(s)

|         | `Ion v1.0`                | `Ion v1.1`                       |
|:--------|:--------------------------|:-------------------------------- |
| **`8`** | `158.10 us` (✅ **1.00x**) | `58.86 us` (🚀 **2.69x faster**)  |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

