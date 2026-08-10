class Test:
    attr = 'hi'

    @cached_property
    def news(self) -> NewsPage:
        return NewsPage(self)

    @cached_property
    def about(self) -> AboutPage:
        return AboutPage(self)
