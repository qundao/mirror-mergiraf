classdef BasicClass
    methods
        function r = fRight(obj)
            r = ceil([obj.Value]);
        end
        function r = f_a(obj,n)
            r = 100*n;
        end
    end
end