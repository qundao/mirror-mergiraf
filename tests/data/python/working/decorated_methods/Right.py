class Test:
    attr = 'hi'

    @cached_property
    def about(self) -> AboutPage:
        return AboutPage(self)
