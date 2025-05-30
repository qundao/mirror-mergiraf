package org.springframework.boot.autoconfigure;

import java.util.List;
import org.junit.jupiter.api.Test;
import org.springframework.boot.autoconfigure.packagestest.one.FirstConfiguration;
import org.springframework.boot.autoconfigure.packagestest.two.SecondConfiguration;
import org.springframework.context.annotation.AnnotationConfigApplicationContext;
import org.springframework.context.annotation.Configuration;
import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatIllegalStateException;

@SuppressWarnings("resource")
public class AutoConfigurationPackagesTests {

    @Test
<<<<<<< LEFT
    void setAndGet() {
        AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(ConfigWithAutoConfigurationPackage.class);
||||||| BASE
    public void setAndGet() {
        AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(ConfigWithRegistrar.class);
=======
    void setAndGet() {
        AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(ConfigWithRegistrar.class);
>>>>>>> RIGHT
        assertThat(AutoConfigurationPackages.get(context.getBeanFactory())).containsExactly(getClass().getPackage().getName());
    }

    @Test
    void getWithoutSet() {
        AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(EmptyConfig.class);
        assertThatIllegalStateException().isThrownBy(() -> AutoConfigurationPackages.get(context.getBeanFactory())).withMessageContaining("Unable to retrieve @EnableAutoConfiguration base packages");
    }

    @Test
    void detectsMultipleAutoConfigurationPackages() {
        AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(FirstConfiguration.class, SecondConfiguration.class);
        List<String> packages = AutoConfigurationPackages.get(context.getBeanFactory());
        Package package1 = FirstConfiguration.class.getPackage();
        Package package2 = SecondConfiguration.class.getPackage();
        assertThat(packages).containsOnly(package1.getName(), package2.getName());
    }

<<<<<<< LEFT
    @Test
    void whenBasePackagesAreSpecifiedThenTheyAreRegistered() {
        AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(ConfigWithAutoConfigurationBasePackages.class);
        List<String> packages = AutoConfigurationPackages.get(context.getBeanFactory());
        assertThat(packages).containsExactly("com.example.alpha", "com.example.bravo");
||||||| BASE
    @Configuration
    @Import(AutoConfigurationPackages.Registrar.class)
    static class ConfigWithRegistrar {
=======
    @Configuration(proxyBeanMethods = false)
    @Import(AutoConfigurationPackages.Registrar.class)
    static class ConfigWithRegistrar {
>>>>>>> RIGHT
    }

<<<<<<< LEFT
    @Test
    void whenBasePackageClassesAreSpecifiedThenTheirPackagesAreRegistered() {
        AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(ConfigWithAutoConfigurationBasePackageClasses.class);
        List<String> packages = AutoConfigurationPackages.get(context.getBeanFactory());
        assertThat(packages).containsOnly(FirstConfiguration.class.getPackage().getName(), SecondConfiguration.class.getPackage().getName());
    }

    @Configuration(proxyBeanMethods = false)
    @AutoConfigurationPackage
    static class ConfigWithAutoConfigurationPackage {
    }

    @Configuration(proxyBeanMethods = false)
    @AutoConfigurationPackage(basePackages = { "com.example.alpha", "com.example.bravo" })
    static class ConfigWithAutoConfigurationBasePackages {
    }

    @Configuration(proxyBeanMethods = false)
    @AutoConfigurationPackage(basePackageClasses = { FirstConfiguration.class, SecondConfiguration.class })
    static class ConfigWithAutoConfigurationBasePackageClasses {
    }

    @Configuration(proxyBeanMethods = false)
||||||| BASE
    @Configuration
=======
    @Configuration(proxyBeanMethods = false)
>>>>>>> RIGHT
    static class EmptyConfig {
    }
}
