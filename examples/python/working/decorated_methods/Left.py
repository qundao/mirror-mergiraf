class Test:
    attr = 'hi'

    @cached_property
    def news(self) -> NewsPage:
        return NewsPage(self)
