use octocrate::{APIConfig, GitHubAPI, PersonalAccessToken, PullRequestReview, PullRequestSimple, PullsSubmitReviewRequest, PullsSubmitReviewRequestEvent, Request, SharedAPIConfig};
use serde::{Deserialize, Serialize};
use tokio::task;

pub type Result<T> = core::result::Result<T, Error>;

pub type Error = Box<dyn std::error::Error>;

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
    let forever = task::spawn(async move {
        loop {
            for git_repo in git_repositories.clone() {
                let f = process_repo(&config, &authors, git_repo).await;
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    });
    forever.await?;
    Ok(())
}

async fn process_repo(config: &SharedAPIConfig, authors: &Vec<String>, git_repo: GitRepo) -> Result<()> {
    println!("processing repo {:?}", git_repo);
    let api: GitHubAPI = GitHubAPI::new(&config);

    let pull_requests: Vec<PullRequestSimple> = api
        .pulls
        .list(git_repo.owner, git_repo.name)
        .send()
        .await?;
    for pull_request in pull_requests {
        if is_pr_for_me(&pull_request, &authors) {
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

fn is_pr_for_me(pr: &PullRequestSimple, authors: &Vec<String>) -> bool {
    let request_me_as_reviewer = match &pr.requested_reviewers {
        Some(reviewers) => {
            reviewers.iter().any(|reviewer| reviewer.login == "barakb")
        }
        None => false,
    };
    let author = pr.user.clone().unwrap().login;
    // println!("author: {}", author);
    let author_is_in_authors = authors.iter().any(|a| a == &author);
    author_is_in_authors && request_me_as_reviewer
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GitRepo {
    owner: String,
    name: String,
}
