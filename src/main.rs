use octocrate::{APIConfig, CheckRunConclusion, ChecksListForRefResponse, GitHubAPI, PersonalAccessToken, PullRequestReview, PullRequestSimple, PullsSubmitReviewRequest, PullsSubmitReviewRequestEvent, Request, SharedAPIConfig};
use serde::{Deserialize, Serialize};

pub type Result<T> = core::result::Result<T, Error>;

pub type Error = Box<dyn std::error::Error>;

const RENOVATE_USER: &str = "renovate[bot]";

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv()?;
    let authors = std::env::var("AUTHORS")?;
    let authors: Vec<String> = serde_json::from_str(&authors)?;
    println!("authors {:?}", authors);
    let git_repositories = std::env::var("GIT_REPOSITORIES")?;
    let git_repositories: Vec<GitRepo> = serde_json::from_str(&git_repositories)?;
    println!("git_repositories {:?}", git_repositories);
    let personal_access_token = std::env::var("GITHUB_PERSONAL_ACCESS_TOKEN")?;
    let personal_access_token = PersonalAccessToken::new(personal_access_token);
    let config = APIConfig::with_token(personal_access_token).shared();
    for git_repo in git_repositories {
        let f = process_repo(&config, &authors, git_repo).await;
    }
    Ok(())
}

async fn process_repo(
    config: &SharedAPIConfig,
    authors: &Vec<String>,
    git_repo: GitRepo,
) -> Result<()> {
    println!("processing repo {:?}", git_repo);
    let api: GitHubAPI = GitHubAPI::new(&config);
    let pull_requests: Vec<PullRequestSimple> =
        api.pulls.list(git_repo.clone().owner, git_repo.clone().name).send().await?;
    for pull_request in pull_requests {
        if is_pr_for_me(&api, &pull_request, &authors).await? {
            println!(
                "found pr review request : '{}' from {} by {}",
                pull_request.title,
                "barakb",
                pull_request.user.clone().unwrap().login
            );
            println!("pr number is {}", pull_request.number);
            println!("name  {:?}", pull_request.base.repo.name);
            println!("owner  {:?}", pull_request.base.repo.owner.login);
            let pr_number = pull_request.number;
            let owner = pull_request.base.repo.owner.login.clone();
            let repo = pull_request.base.repo.name.clone();
            let all_reviews: Vec<PullRequestReview> = api
                .pulls
                .list_reviews(owner.clone(), repo.clone(), pr_number)
                .send()
                .await?;
            // if there is no review for me create one
            if let Some(review) = has_review_for_me(&all_reviews) {
                println!("review already exists for me {} ", review.id);
                approve(&api, &pull_request, review).send().await?;
            } else {
                println!("creating a fresh pr review for pr {}", pr_number);
                let review = api
                    .pulls
                    .create_review(owner, repo, pr_number)
                    .send()
                    .await?;
                approve(&api, &pull_request, &review).send().await?;
            }
        }
    }
    Ok(())
}

fn approve(
    api: &GitHubAPI,
    pr: &PullRequestSimple,
    review: &PullRequestReview,
) -> Request<PullsSubmitReviewRequest, (), PullRequestReview> {
    println!("approving pr {}", pr.number);
    let approve = PullsSubmitReviewRequest {
        body: None,
        event: PullsSubmitReviewRequestEvent::Approve,
    };
    let owner = pr.base.repo.owner.login.clone();
    let repo = pr.base.repo.name.clone();
    api.pulls
        .submit_review(owner, repo, pr.number, review.id)
        .body(&approve)
}

fn has_review_for_me(
    reviews: &Vec<PullRequestReview>,
) -> Option<&PullRequestReview> {
    for review in reviews {
        if review.clone().user.unwrap().login == "barakb"
            && review.state == "PENDING"
        {
            return Some(review);
        }
    }
    None
}

async fn is_pr_for_me(api: &GitHubAPI, pr: &PullRequestSimple, authors: &Vec<String>) -> Result<bool> {
    let owner = pr.base.repo.owner.login.clone();
    let repo = pr.base.repo.name.clone();
    if (pr.user.clone().unwrap().login == RENOVATE_USER && is_none_or_empty(pr.clone().requested_reviewers)) {
        let all_checks_good = api.checks.list_for_ref(owner.clone(), repo.clone(), pr.head.sha.clone())
            .send().await?.check_runs.iter().all(|check|
            check.conclusion == Some(CheckRunConclusion::Success)
                || check.conclusion == Some(CheckRunConclusion::Skipped)
        );
        let no_reviews = api.pulls.list_reviews(owner.clone(), repo.clone(), pr.number).send().await?.is_empty();
        if (all_checks_good && no_reviews) {
            println!(
                "renovate user without review request found: {}, pr {} on repo {} all checks done: {}",
                pr.title, pr.number, pr.base.repo.name, all_checks_good
            );
            let f = api.pulls.request_reviewers(owner.clone(), repo.clone(), pr.number)
                .body(&serde_json::json!({"reviewers": ["barakb"]}))
                .send()
                .await?;
            return Ok(true);
        }
    }

    let request_me_as_reviewer = match &pr.requested_reviewers {
        Some(reviewers) => {
            reviewers.iter().any(|reviewer| reviewer.login == "barakb")
        }
        None => false,
    };
    let author = pr.user.clone().unwrap().login;

    let author_is_in_authors = authors.iter().any(|a| a == &author);
    Ok(author_is_in_authors && request_me_as_reviewer)
}

fn is_none_or_empty<T>(reviewers: Option<Vec<T>>) -> bool {
    match reviewers {
        Some(reviewers) => reviewers.is_empty(),
        None => true,
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GitRepo {
    owner: String,
    name: String,
}
