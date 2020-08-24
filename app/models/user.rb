# == Schema Information
#
# Table name: users
#
#  id              :bigint           not null, primary key
#  email           :string           not null
#  p_num           :string           not null
#  password_digest :string           not null
#  session_token   :string           not null
#  created_at      :datetime         not null
#  updated_at      :datetime         not null
#
# Indexes
#
#  index_users_on_email          (email) UNIQUE
#  index_users_on_session_token  (session_token)
#
class User < ApplicationRecord
  validates :password_digest, :session_token, presence: true
  validates :email, :p_num, uniqueness: true
  validates :p_num, presence: { message: "Please enter a valid phone number" }
  validates :email, presence: { message: "Please enter a valid email" }
  validates :password, length: { minimum: 6, message: "Your password must be at least 6 characters long", allow_nil: true }
  validates :p_num, length: { minimum: 10, maximum: 11 }
  after_initialize :ensure_session_token
  attr_reader :password 

  has_many :accounts, 
    foreign_key: :user_id,
    class_name: :Account

  # has_many :bills,
  # foreign_key: :user_id,
  # class_name: :Bill

  has_many :transactions,
  through: :accounts, 
  source: :transactions 
  
  has_many :goals,
  through: :accounts,
  source: :goals


  
  def self.find_by_credentials(em, pw) 
    @user = User.find_by(email: em);
    if @user && @user.is_password?(pw) 
      @user 
    else
      nil
    end
  end

  def password=(pw) 
    self.password_digest = BCrypt::Password.create(pw)
    @password = pw
  end

  def is_password?(pw) 
    BCrypt::Password.new(self.password_digest).is_password?(pw)
  end

  def reset_session_token!
    self.session_token = SecureRandom::urlsafe_base64
    self.save
    self.session_token
  end

  def ensure_session_token
    self.session_token ||= SecureRandom::urlsafe_base64
  end
end
