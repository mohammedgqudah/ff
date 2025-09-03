# creating a drive partition for benchmarking and testing

`ff` needs a partition that will be used to do all sorts of tests and benchmarks, it will refuse to work if the partition label is not `ff-bench`, which can be set using `parted`

```sh
sudo parted /dev/<YOUR_DRIVE> name <TESTING_PART_NUMBER> ff-bench
```
