int main() {
  {
    int x = 5;
    int *p = &x;
    *p = 10; // fine, p and x are in same scope
  }
  return 0;
}
