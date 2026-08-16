classdef BasicClass
    methods
        function r = fRight(obj)
            r = ceil([obj.Value],2);
        end
        function r = f_a(obj,n)
            r = 100*n;
        end
    end
end