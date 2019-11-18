//
// Created by 付旭炜 on 2019/3/24.
//

#pragma "once"

#include <iostream>
#include <vector>


struct TreeNode {
      int val;
      TreeNode *left;
      TreeNode *right;
      TreeNode(int x) : val(x), left(NULL), right(NULL) {}
};


template<typename T>
void print_perms(const std::vector<T>& perms) {
    for (int i = 0; i < perms.size(); ++i) {
        std::cout << perms[i] << ' ';
    }
    std::cout << '\n';
}

//TreeNode* make_list(std::initializer_list<int > val_list) {
//
//}

