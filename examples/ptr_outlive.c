int main() {
    int *p;
    {
        int x = 5;
        p = &x;
    }
    *p = 10;
    return 0;
}
