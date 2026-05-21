
int *foo() {
  int x = 5;
  return &x; // error — returning stack address
}

int main() {
  int *p = foo();
  return 0;
}
