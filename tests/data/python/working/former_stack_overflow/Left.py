@task()
class RunJob:
    

    
    

    

    

    def build_playbook_path_relative_to_cwd(self, job, private_data_dir):

    def pre_run_hook(self, job,):
        
        job_revision = job.project.scm_revision
        all_sync_needs = ['update_{}'.format(job.project.scm_type), 'install_roles']
        if not job.project.scm_type:
            pass
        elif not os.path.exists(project_path):
            logger.debug('Performing fresh clone of {} on this instance.'.format(job.project))
            sync_needs = all_sync_needs
        elif not job.project.scm_revision:
            logger.debug('Revision not known for {}, will sync with remote'.format(job.project))
            sync_needs = all_sync_needs
        elif job.project.scm_type == 'git':
            git_repo = git.Repo(project_path)
            try:
                desired_revision = job.project.scm_revision
                if job.scm_branch and job.scm_branch != job.project.scm_branch:
                    desired_revision = job.scm_branch  # could be commit or not, but will try as commit
                current_revision = git_repo.head.commit.hexsha
                if desired_revision == current_revision:
                    job_revision = desired_revision
                    logger.info('Skipping project sync for {} because commit is locally available'.format(job.log_format))
                else:
                    sync_needs = all_sync_needs
            except (ValueError, BadGitName):
                logger.debug('Needed commit for {} not in local source tree, will sync with remote'.format(job.log_format))
                sync_needs = all_sync_needs
        # Galaxy requirements are not supported for manual projects

        if sync_needs:
            pu_ig = job.instance_group
            pu_en = job.execution_node
            if job.is_isolated() is True:
                pu_ig = pu_ig.controller
                pu_en = settings.CLUSTER_HOST_ID

            sync_metafields = dict(
                launch_type="sync",
                job_type='run',
                job_tags=','.join(sync_needs),
                status='running',
                instance_group = pu_ig,
                execution_node=pu_en,
                celery_task_id=job.celery_task_id
            )
            if job.scm_branch and job.scm_branch != job.project.scm_branch:
                sync_metafields['scm_branch'] = job.scm_branch
            if 'update_' not in sync_metafields['job_tags']:
                sync_metafields['scm_revision'] = job_revision
            local_project_sync = job.project.create_project_update(_eager_fields=sync_metafields)
            # save the associated job before calling run() so that a
            # cancel() call on the job can cancel the project update
            job = self.update_model(job.pk, project_update=local_project_sync)

            project_update_task = local_project_sync._get_task_class()
            try:
                # the job private_data_dir is passed so sync can download roles and collections there
                sync_task = project_update_task(job_private_data_dir=private_data_dir)
                sync_task.run(local_project_sync.id)
                local_project_sync.refresh_from_db()
                job = self.update_model(job.pk, scm_revision=local_project_sync.scm_revision)
            except Exception:
                local_project_sync.refresh_from_db()
                if local_project_sync.status != 'canceled':
                    job = self.update_model(job.pk, status='failed',
                                            job_explanation=('Previous Task Failed: {"job_type": "%s", "job_name": "%s", "job_id": "%s"}' %
                                                             ('project_update', local_project_sync.name, local_project_sync.id)))
                    raise
                job.refresh_from_db()
                if job.cancel_flag:
                    return
        else:
            # up-to-date with project, job is running project current version
            if job_revision:
                job = self.update_model(job.pk, scm_revision=job_revision)
            # Project update does not copy the folder, so copy here
            RunProjectUpdate.make_local_copy(
                project_path, os.path.join(private_data_dir, 'project'),
                job.project.scm_type, job_revision
            )

        if job.inventory.kind == 'smart':
            # cache smart inventory memberships so that the host_filter query is not
            # ran inside of the event saving code
            update_smart_memberships_for_inventory(job.inventory)


@task()
class RunProjectUpdate:

    

    

    

    
        

    

    def pre_run_hook(self, instance, private_data_dir):
        
        self.acquire_lock(instance)
        if (instance.scm_type == 'git' and instance.job_type == 'run' and instance.project and
                instance.scm_branch != instance.project.scm_branch):
            project_path = instance.project.get_project_path(check_if_exists=False)
            if os.path.exists(project_path):
                git_repo = git.Repo(project_path)
                if git_repo.head.is_detached:
                    self.original_branch = git_repo.head.commit
                else:
                    self.original_branch = git_repo.active_branch

    def post_run_hook():
        try:
            if self.playbook_new_revision:
                instance.scm_revision = self.playbook_new_revision
                instance.save(update_fields=['scm_revision'])
            if self.job_private_data_dir:
                # copy project folder before resetting to default branch
                # because some git-tree-specific resources (like submodules) might matter
                self.make_local_copy(
                    instance.get_project_path(check_if_exists=False), os.path.join(self.job_private_data_dir, 'project'),
                    instance.scm_type, instance.scm_revision
                )
                if self.original_branch:
                    # for git project syncs, non-default branches can be problems
                    # restore to branch the repo was on before this run
                    try:
                        self.original_branch.checkout()
                    except Exception:
                        # this could have failed due to dirty tree, but difficult to predict all cases
                        logger.exception('Failed to restore project repo to prior state after {}'.format(instance.log_format))
        finally:
        p = instance.project
        if instance.job_type == 'check' and status not in ('failed', 'canceled',):
            if self.playbook_new_revision:
                p.scm_revision = self.playbook_new_revision
            else:
                if status == 'successful':
                    logger.error("{} Could not find scm revision in check".format(instance.log_format))
            p.playbook_files = p.playbooks
            p.inventory_files = p.inventories
            p.save(update_fields=['scm_revision', 'playbook_files', 'inventory_files'])

        # Update any inventories that depend on this project
        dependent_inventory_sources = p.scm_inventory_sources.filter(update_on_project_update=True)
        if len(dependent_inventory_sources) > 0:
            if status == 'successful' and instance.launch_type != 'sync':
                self._update_dependent_inventories(instance, dependent_inventory_sources)